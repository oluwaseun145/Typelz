use std::path::Path;
use std::sync::{Arc, Mutex};

use ort::session::{Session, SessionInputs};
use ort::value::Tensor;

pub fn load_vocab(path: &Path) -> Result<Vec<String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read vocab file {}: {e}", path.display()))?;
    Ok(content.lines().map(|l| l.to_string()).collect())
}

/// Decode CTC-style token sequence to text, removing blanks and repetitions.
fn ctc_decode(tokens: &[usize], vocab: &[String]) -> String {
    const BLANK_TOKEN: usize = 0;
    let mut result = String::new();
    let mut prev_token = BLANK_TOKEN;

    for &token in tokens {
        if token == BLANK_TOKEN || token >= vocab.len() {
            continue;
        }
        if token != prev_token {
            let token_str = &vocab[token];
            let cleaned: String = token_str.chars()
                .filter(|c| !matches!(c, '\u{2581}' | '<' | '>'))
                .collect();
            if !cleaned.is_empty() {
                result.push_str(&cleaned);
            }
        }
        prev_token = token;
    }

    let mut chars: String = result.chars().collect();
    if !chars.is_empty() {
        let first: String = chars.drain(..1).collect::<String>().to_uppercase();
        result = format!("{}{}", first, chars);
    }
    if !result.ends_with('.') && !result.ends_with(',') && !result.ends_with('!')
        && !result.ends_with('?') && !result.ends_with(':') && !result.ends_with(';') {
        result.push('.');
    }

    result
}

/// Narrow beam search over per-frame token scores.
///
/// Each frame contributes one token to the output sequence and each beam
/// carries a cumulative log-score. After every frame, beams that fall more
/// than `threshold` behind the best beam are pruned before the top
/// `beam_width` are kept (F-19), bounding the work spent on dead-end paths.
/// Returns the tokens of the best-scoring sequence.
fn beam_search(
    frames: &[&[f32]],
    vocab_size: usize,
    beam_width: usize,
    threshold: f32,
) -> Vec<usize> {
    if beam_width == 0 || frames.is_empty() {
        return Vec::new();
    }

    let mut beams: Vec<(f32, Vec<usize>)> = vec![(0.0, Vec::new()); beam_width];

    for frame in frames {
        // Scores beyond the frame length are treated as -inf so a short frame
        // degrades instead of reading out of bounds.
        let mut scores: Vec<(f32, usize)> = (0..vocab_size)
            .map(|k| (frame.get(k).copied().unwrap_or(f32::NEG_INFINITY), k))
            .collect();
        scores.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if scores.is_empty() {
            break;
        }

        let mut next_beams: Vec<(f32, Vec<usize>)> = Vec::with_capacity(beams.len() * beam_width);
        for (beam_score, beam_tokens) in &beams {
            // Expand each beam with this frame's best-scoring tokens.
            for &(score, tok_idx) in scores.iter().rev().take(beam_width) {
                let new_score = beam_score + score;
                let mut new_tokens = beam_tokens.clone();
                new_tokens.push(tok_idx);
                next_beams.push((new_score, new_tokens));
            }
        }
        if next_beams.is_empty() {
            break;
        }

        next_beams.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let best = next_beams.last().map(|(s, _)| *s).unwrap_or(f32::NEG_INFINITY);
        beams = next_beams
            .into_iter()
            .rev()
            .filter(|(score, _)| *score >= best - threshold)
            .take(beam_width)
            .collect();
    }

    // Beams are kept best-first, so the head is the winning sequence.
    beams.into_iter().next().map(|(_, tokens)| tokens).unwrap_or_default()
}

/// Transcription error type.
#[derive(Debug)]
pub enum TranscribeError {
    ModelLoad(String),
    Inference(String),
    AudioProcessing(String),
    ShapeMismatch { expected: usize, actual: usize },
}

impl std::fmt::Display for TranscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscribeError::ModelLoad(msg) => write!(f, "Model load error: {msg}"),
            TranscribeError::Inference(msg) => write!(f, "Inference error: {msg}"),
            TranscribeError::AudioProcessing(msg) => write!(f, "Audio processing error: {msg}"),
            TranscribeError::ShapeMismatch { expected, actual } => {
                write!(f, "Tensor shape mismatch: expected dim {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for TranscribeError {}

/// Parukeet TDT v3 transcription engine using ONNX Runtime.
///
/// Only the encoder session is loaded. The decoder_joint session is
/// intentionally not loaded (F-17): wiring it requires the TDT transducer
/// loop (fbank features, hidden states, duration-based token emission), which
/// is a follow-up spec item — see the contract note in `run_inference`.
pub struct TranscriptionEngine {
    encoder: Arc<Mutex<Session>>,
    vocab: Vec<String>,
    hidden_dim: usize,
    last_transcript: Mutex<String>,
}

impl TranscriptionEngine {
    /// Creates a new transcription engine by loading the ONNX models.
    pub fn new(model_dir: &Path) -> Result<Self, TranscribeError> {
        let encoder_path = model_dir.join("encoder-model.int8.onnx");
        let vocab_path = model_dir.join("vocab.txt");

        let vocab = load_vocab(&vocab_path).map_err(|e| TranscribeError::ModelLoad(e))?;

        let mut builder = Session::builder().map_err(|e| {
            TranscribeError::ModelLoad(format!("Failed to create session builder: {e}"))
        })?;
        let encoder_session = builder.commit_from_file(encoder_path).map_err(|e| {
            TranscribeError::ModelLoad(format!("Failed to load encoder: {e}"))
        })?;

        // Validate encoder output shape and extract hidden dimension.
        // The encoder outputs logits with shape [batch, time_steps, hidden_dim].
        const EXPECTED_HIDDEN_DIM: usize = 1024;
        let hidden_dim = EXPECTED_HIDDEN_DIM; // validated at runtime in run_inference

        Ok(Self {
            encoder: Arc::new(Mutex::new(encoder_session)),
            vocab,
            hidden_dim,
            last_transcript: Mutex::new(String::new()),
        })
    }

    /// Returns the last transcript.
    pub fn last_transcript(&self) -> Option<String> {
        self.last_transcript.lock().ok().map(|s| s.clone())
    }

    /// Transcribes a 16 kHz mono i16 little-endian PCM utterance to text.
    pub fn transcribe(&self, pcm: &[u8]) -> Result<String, TranscribeError> {
        let samples: Vec<f32> = pcm
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect();

        if samples.is_empty() {
            return Err(TranscribeError::AudioProcessing("Empty audio".to_string()));
        }

        let text = self.run_inference(&samples)?;

        {
            let mut last = self.last_transcript.lock().unwrap();
            *last = text.clone();
        }

        Ok(text)
    }

    /// Runs the full inference pipeline: encoder → beam search → CTC decode.
    ///
    /// NOTE (F-17): the verified ONNX contract for this model pair (repo
    /// `istupakov/parakeet-tdt-0.6b-v3-onnx`; reference implementation
    /// `onnx-asr`, `models/nemo.py`, `NemoConformerTdt`) is:
    ///   - encoder inputs: `audio_signal` [1, 80, T] f32 (10 ms log-mel fbank,
    ///     100 frames/s at 16 kHz) + `length` [1] i64
    ///   - encoder outputs: `outputs` [1, H, T'] f32 + `encoded_lengths` [1] i64
    ///   - decoder_joint inputs: `encoder_outputs`, `targets`, `target_length`,
    ///     `input_states_1`, `input_states_2`
    ///   - TDT decoding: iterative — the joint output is
    ///     [vocab logits | duration scores] and each emitted token consumes
    ///     the predicted number of frames.
    /// The code below is a placeholder: it feeds raw 16 kHz PCM as a single
    /// `input` tensor and treats encoder output rows as token scores. It runs
    /// end-to-end but will not produce correct transcripts against the real
    /// models until a follow-up spec wires fbank features and the TDT
    /// transducer loop (including the decoder_joint session).
    fn run_inference(&self, samples: &[f32]) -> Result<String, TranscribeError> {
        const BEAM_WIDTH: usize = 5;
        // Drop candidate beams more than this log-score behind the best one.
        const BEAM_THRESHOLD: f32 = 10.0;

        let tensor = Tensor::from_array((vec![samples.len() as i64], samples.to_vec()))
            .map_err(|e| TranscribeError::Inference(format!("Failed to create input tensor: {e}")))?;
        let mut inputs: std::collections::HashMap<String, ort::value::DynTensor> = std::collections::HashMap::new();
        inputs.insert("input".to_string(), tensor.upcast());

        let data: Vec<f32> = {
            let mut enc = self.encoder.lock().unwrap();
            let outputs = enc.run(SessionInputs::from(inputs))
                .map_err(|e| TranscribeError::Inference(format!("Encoder inference failed: {e}")))?;

            let value_ref = outputs.values().next()
                .ok_or_else(|| TranscribeError::Inference("No encoder output".to_string()))?;

            let array = value_ref.try_extract_array::<f32>()
                .map_err(|_| TranscribeError::Inference("Failed to extract tensor data".to_string()))?;

            array.as_slice()
                .map(|s| s.to_vec())
                .ok_or_else(|| TranscribeError::Inference("Tensor not contiguous".to_string()))?
        };

        let hidden_dim = self.hidden_dim;
        if data.len() % hidden_dim != 0 {
            return Err(TranscribeError::ShapeMismatch {
                expected: hidden_dim,
                actual: data.len() % hidden_dim,
            });
        }

        let frames: Vec<&[f32]> = data.chunks(hidden_dim).collect();
        let best_tokens = beam_search(&frames, self.vocab.len(), BEAM_WIDTH, BEAM_THRESHOLD);
        let text = ctc_decode(&best_tokens, &self.vocab);
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctc_decode_removes_blanks() {
        let vocab = vec![
            "<blank>".to_string(),
            "▁Hello".to_string(),
            "▁World".to_string(),
        ];
        let tokens = vec![0, 1, 1, 0, 2, 0];
        let result = ctc_decode(&tokens, &vocab);
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
    }

    #[test]
    fn test_ctc_decode_all_blank() {
        let vocab = vec![
            "<blank>".to_string(),
            "▁cat".to_string(),
        ];
        let tokens = vec![0, 0, 0, 0];
        let result = ctc_decode(&tokens, &vocab);
        assert_eq!(result, ".");
    }

    #[test]
    fn test_ctc_decode_repeated_tokens() {
        let vocab = vec![
            "<blank>".to_string(),
            "▁hello".to_string(),
        ];
        let tokens = vec![1, 1, 1, 0];
        let result = ctc_decode(&tokens, &vocab);
        assert_eq!(result, "Hello.");
    }

    #[test]
    fn test_load_vocab_invalid_path() {
        let result = load_vocab(Path::new("/nonexistent/vocab.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_beam_search_picks_best_path() {
        let f1 = vec![0.0, 5.0, 1.0];
        let f2 = vec![2.0, 3.0, 0.0];
        let frames = vec![f1.as_slice(), f2.as_slice()];
        assert_eq!(beam_search(&frames, 3, 3, 10.0), vec![1, 1]);
    }

    #[test]
    fn test_beam_search_picks_least_bad_when_all_negative() {
        let f1 = vec![-5.0, -1.0, -9.0];
        let frames = vec![f1.as_slice()];
        assert_eq!(beam_search(&frames, 3, 3, 10.0), vec![1]);
    }

    #[test]
    fn test_beam_search_handles_frames_shorter_than_vocab() {
        // Token scores beyond the frame length are -inf, so a short frame
        // must not panic or read out of bounds.
        let f1 = vec![0.0, -1.0];
        let frames = vec![f1.as_slice()];
        assert_eq!(beam_search(&frames, 8, 3, 10.0), vec![0]);
    }

    #[test]
    fn test_beam_search_empty_inputs() {
        assert!(beam_search(&[], 3, 3, 10.0).is_empty());
        assert!(beam_search(&[&[0.0f32]], 3, 0, 10.0).is_empty());
    }
}
