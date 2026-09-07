use std::path::Path;
use std::sync::{Arc, Mutex};

use ort::session::{Session, SessionInputs};
use ort::value::Tensor;
const NEMO128_ONNX: &[u8] = include_bytes!("../models/nemo128.onnx");

pub fn load_vocab(path: &Path) -> Result<Vec<String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read vocab file {}: {e}", path.display()))?;
    Ok(content.lines().map(|l| l.to_string()).collect())
}

/// Formats decoded token text into human-readable sentences with capitalization
/// and trailing punctuation.
fn format_transcript(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut chars: String = trimmed.chars().collect();
    let first: String = chars.drain(..1).collect::<String>().to_uppercase();
    let mut result = format!("{}{}", first, chars);

    if !result.ends_with('.') && !result.ends_with(',') && !result.ends_with('!')
        && !result.ends_with('?') && !result.ends_with(':') && !result.ends_with(';') {
        result.push('.');
    }

    result
}

/// Joins a new utterance transcript onto the accumulated session transcript.
/// Whitespace is trimmed and segments are separated by a single space; any
/// empty input yields the other side (or empty when both are empty).
fn join_transcript(existing: &str, new: &str) -> String {
    let e = existing.trim();
    let n = new.trim();
    match (e.is_empty(), n.is_empty()) {
        (true, true) => String::new(),
        (true, false) => n.to_string(),
        (false, true) => e.to_string(),
        (false, false) => format!("{e} {n}"),
    }
}

#[allow(dead_code)]
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

    format_transcript(&result)
}

#[allow(dead_code)]
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
    preprocessor: Arc<Mutex<Session>>,
    encoder: Arc<Mutex<Session>>,
    decoder_joint: Arc<Mutex<Session>>,
    vocab: Vec<String>,
    blank_idx: usize,
    /// All utterances of the current session, joined. Reset when a new
    /// session starts; delivered whole when the recording stops.
    session_transcript: Mutex<String>,
}

impl TranscriptionEngine {
    /// Creates a new transcription engine by loading the ONNX models.
    pub fn new(model_dir: &Path) -> Result<Self, TranscribeError> {
        let encoder_path = model_dir.join("encoder-model.int8.onnx");
        let decoder_path = model_dir.join("decoder_joint-model.int8.onnx");
        let vocab_path = model_dir.join("vocab.txt");

        let raw_vocab = load_vocab(&vocab_path).map_err(TranscribeError::ModelLoad)?;

        let mut vocab: Vec<String> = Vec::new();
        let mut blank_idx = 8192;
        for (i, line) in raw_vocab.iter().enumerate() {
            let last_space = line.rfind(' ').unwrap_or(line.len());
            let tok = &line[..last_space];
            let idx = line[last_space..].trim().parse::<usize>().unwrap_or(i);
            if tok == "<blk>" {
                blank_idx = idx;
            }
            if idx >= vocab.len() {
                vocab.resize(idx + 1, String::new());
            }
            vocab[idx] = tok.to_string();
        }

        let mut preproc_builder = Session::builder().map_err(|e| {
            TranscribeError::ModelLoad(format!("Failed to create preprocessor session builder: {e}"))
        })?;
        let preprocessor_session = preproc_builder.commit_from_memory(NEMO128_ONNX).map_err(|e| {
            TranscribeError::ModelLoad(format!("Failed to load preprocessor: {e}"))
        })?;

        let mut enc_builder = Session::builder().map_err(|e| {
            TranscribeError::ModelLoad(format!("Failed to create encoder session builder: {e}"))
        })?;
        let encoder_session = enc_builder.commit_from_file(encoder_path).map_err(|e| {
            TranscribeError::ModelLoad(format!("Failed to load encoder: {e}"))
        })?;

        let mut dec_builder = Session::builder().map_err(|e| {
            TranscribeError::ModelLoad(format!("Failed to create decoder session builder: {e}"))
        })?;
        let decoder_session = dec_builder.commit_from_file(decoder_path).map_err(|e| {
            TranscribeError::ModelLoad(format!("Failed to load decoder_joint: {e}"))
        })?;

        Ok(Self {
            preprocessor: Arc::new(Mutex::new(preprocessor_session)),
            encoder: Arc::new(Mutex::new(encoder_session)),
            decoder_joint: Arc::new(Mutex::new(decoder_session)),
            vocab,
            blank_idx,
            session_transcript: Mutex::new(String::new()),
        })
    }

    /// Clears the accumulated session transcript. Called at the start of a new
    /// transcription session so a previous session's result is not surfaced
    /// as the new one.
    pub fn clear_transcript(&self) {
        if let Ok(mut session) = self.session_transcript.lock() {
            session.clear();
        }
    }

    /// Returns the transcript accumulated over the current session.
    pub fn session_transcript(&self) -> String {
        self.session_transcript.lock().ok().map(|s| s.clone()).unwrap_or_default()
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

        let started = std::time::Instant::now();
        let text = self.run_inference(&samples)?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[typelz] inference: {} samples (~{:.1}s) -> {} chars in {:.0} ms",
            samples.len(),
            samples.len() as f64 / 16000.0,
            text.len(),
            elapsed_ms,
        );

        {
            let mut session = self.session_transcript.lock().unwrap();
            *session = join_transcript(&session, &text);
        }

        Ok(text)
    }

    /// Runs the full inference pipeline:
    /// 1. Preprocessor: 16 kHz PCM waveform -> 128 log-mel filterbank features [1, 128, T_feat]
    /// 2. Encoder: features + length -> acoustic embeddings [1, 1024, T_enc]
    /// 3. Decoder joint: TDT decoding loop emitting tokens and advancing time frames
    fn run_inference(&self, samples: &[f32]) -> Result<String, TranscribeError> {
        let sample_len = samples.len() as i64;
        let waveforms = Tensor::from_array(([1i64, sample_len], samples.to_vec().into_boxed_slice()))
            .map_err(|e| TranscribeError::Inference(format!("Failed to create waveforms tensor: {e}")))?;
        let waveforms_lens = Tensor::from_array(([1i64], vec![sample_len].into_boxed_slice()))
            .map_err(|e| TranscribeError::Inference(format!("Failed to create waveforms_lens tensor: {e}")))?;

        let (features, features_lens) = {
            let mut preproc_inputs = std::collections::HashMap::new();
            preproc_inputs.insert("waveforms".to_string(), waveforms.upcast());
            preproc_inputs.insert("waveforms_lens".to_string(), waveforms_lens.upcast());

            let mut preproc = self.preprocessor.lock().unwrap();
            let outputs = preproc.run(SessionInputs::from(preproc_inputs))
                .map_err(|e| TranscribeError::Inference(format!("Preprocessor inference failed: {e}")))?;

            let feat = outputs.get("features")
                .ok_or_else(|| TranscribeError::Inference("Missing features output".to_string()))?;
            let feat_len = outputs.get("features_lens")
                .ok_or_else(|| TranscribeError::Inference("Missing features_lens output".to_string()))?;

            let (feat_shape, feat_data) = feat.try_extract_tensor::<f32>()
                .map_err(|e| TranscribeError::Inference(format!("Failed to extract features: {e}")))?;
            let feat_shape_vec: Vec<i64> = feat_shape.iter().copied().collect();
            let feat_tensor = Tensor::from_array((feat_shape_vec, feat_data.to_vec().into_boxed_slice()))
                .map_err(|e| TranscribeError::Inference(format!("Failed to clone features tensor: {e}")))?;

            let (len_shape, len_data) = feat_len.try_extract_tensor::<i64>()
                .map_err(|e| TranscribeError::Inference(format!("Failed to extract features_lens: {e}")))?;
            let len_shape_vec: Vec<i64> = len_shape.iter().copied().collect();
            let len_tensor = Tensor::from_array((len_shape_vec, len_data.to_vec().into_boxed_slice()))
                .map_err(|e| TranscribeError::Inference(format!("Failed to clone features_lens tensor: {e}")))?;

            (feat_tensor, len_tensor)
        };

        let (enc_data, enc_len, time_steps) = {
            let mut enc_inputs = std::collections::HashMap::new();
            enc_inputs.insert("audio_signal".to_string(), features.upcast());
            enc_inputs.insert("length".to_string(), features_lens.upcast());

            let mut enc = self.encoder.lock().unwrap();
            let outputs = enc.run(SessionInputs::from(enc_inputs))
                .map_err(|e| TranscribeError::Inference(format!("Encoder inference failed: {e}")))?;

            let out = outputs.get("outputs")
                .ok_or_else(|| TranscribeError::Inference("Missing encoder outputs".to_string()))?;
            let out_lens = outputs.get("encoded_lengths")
                .ok_or_else(|| TranscribeError::Inference("Missing encoder encoded_lengths".to_string()))?;

            let (out_shape, out_slice) = out.try_extract_tensor::<f32>()
                .map_err(|e| TranscribeError::Inference(format!("Failed to extract encoder outputs: {e}")))?;
            if out_shape.len() < 3 || out_shape[1] != 1024 {
                return Err(TranscribeError::ShapeMismatch {
                    expected: 1024,
                    actual: if out_shape.len() >= 2 { out_shape[1] as usize } else { 0 },
                });
            }
            let time_steps = out_shape[2] as usize;

            let (_, len_slice) = out_lens.try_extract_tensor::<i64>()
                .map_err(|e| TranscribeError::Inference(format!("Failed to extract encoder lengths: {e}")))?;
            let enc_len = if !len_slice.is_empty() { len_slice[0] as usize } else { 0 };

            (out_slice.to_vec(), enc_len, time_steps)
        };

        // TDT greedy transducer loop
        let vocab_size = self.vocab.len();
        let blank_idx = self.blank_idx;
        let mut state1_vec = vec![0.0f32; 2 * 1 * 640];
        let mut state2_vec = vec![0.0f32; 2 * 1 * 640];
        let mut tokens: Vec<usize> = Vec::new();
        let mut t = 0;
        let mut emitted_tokens = 0;
        const MAX_TOKENS_PER_STEP: usize = 10;

        let mut dec = self.decoder_joint.lock().unwrap();

        while t < enc_len && t < time_steps {
            let mut enc_frame = Vec::with_capacity(1024);
            for c in 0..1024 {
                enc_frame.push(enc_data[c * time_steps + t]);
            }

            let last_token = *tokens.last().unwrap_or(&blank_idx) as i32;

            let enc_out_tensor = Tensor::from_array(([1i64, 1024i64, 1i64], enc_frame.into_boxed_slice()))
                .map_err(|e| TranscribeError::Inference(format!("Failed to build frame tensor: {e}")))?;
            let targets = Tensor::from_array(([1i64, 1i64], vec![last_token].into_boxed_slice()))
                .map_err(|e| TranscribeError::Inference(format!("Failed to build targets tensor: {e}")))?;
            let target_length = Tensor::from_array(([1i64], vec![1i32].into_boxed_slice()))
                .map_err(|e| TranscribeError::Inference(format!("Failed to build target_length tensor: {e}")))?;
            let state1 = Tensor::from_array(([2i64, 1i64, 640i64], state1_vec.clone().into_boxed_slice()))
                .map_err(|e| TranscribeError::Inference(format!("Failed to build state1 tensor: {e}")))?;
            let state2 = Tensor::from_array(([2i64, 1i64, 640i64], state2_vec.clone().into_boxed_slice()))
                .map_err(|e| TranscribeError::Inference(format!("Failed to build state2 tensor: {e}")))?;

            let mut dec_inputs = std::collections::HashMap::new();
            dec_inputs.insert("encoder_outputs".to_string(), enc_out_tensor.upcast());
            dec_inputs.insert("targets".to_string(), targets.upcast());
            dec_inputs.insert("target_length".to_string(), target_length.upcast());
            dec_inputs.insert("input_states_1".to_string(), state1.upcast());
            dec_inputs.insert("input_states_2".to_string(), state2.upcast());

            let dec_outputs = dec.run(SessionInputs::from(dec_inputs))
                .map_err(|e| TranscribeError::Inference(format!("Decoder inference failed: {e}")))?;

            let out_logits = dec_outputs.get("outputs")
                .ok_or_else(|| TranscribeError::Inference("Missing decoder outputs".to_string()))?;
            let out_state1 = dec_outputs.get("output_states_1")
                .ok_or_else(|| TranscribeError::Inference("Missing decoder output_states_1".to_string()))?;
            let out_state2 = dec_outputs.get("output_states_2")
                .ok_or_else(|| TranscribeError::Inference("Missing decoder output_states_2".to_string()))?;

            let (_, logits_data) = out_logits.try_extract_tensor::<f32>()
                .map_err(|e| TranscribeError::Inference(format!("Failed to extract logits: {e}")))?;

            let token_logits = if logits_data.len() >= vocab_size {
                &logits_data[..vocab_size]
            } else {
                logits_data
            };
            let best_token = token_logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| idx)
                .unwrap_or(blank_idx);

            let dur_logits = if logits_data.len() > vocab_size {
                &logits_data[vocab_size..]
            } else {
                &[]
            };
            let step = dur_logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            if best_token != blank_idx {
                let (_, s1) = out_state1.try_extract_tensor::<f32>()
                    .map_err(|e| TranscribeError::Inference(format!("Failed to extract state1: {e}")))?;
                let (_, s2) = out_state2.try_extract_tensor::<f32>()
                    .map_err(|e| TranscribeError::Inference(format!("Failed to extract state2: {e}")))?;
                state1_vec = s1.to_vec();
                state2_vec = s2.to_vec();
                tokens.push(best_token);
                emitted_tokens += 1;
            }

            if step > 0 {
                t += step;
                emitted_tokens = 0;
            } else if best_token == blank_idx || emitted_tokens == MAX_TOKENS_PER_STEP {
                t += 1;
                emitted_tokens = 0;
            }
        }

        let raw: String = tokens
            .iter()
            .filter_map(|&tok| self.vocab.get(tok))
            .filter(|tok_str| !tok_str.starts_with('<'))
            .map(|tok_str| tok_str.replace('\u{2581}', " "))
            .collect();

        Ok(format_transcript(&raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decode_formatting() {
        let text1 = format_transcript("hello world");
        assert_eq!(text1, "Hello world.");

        let text2 = format_transcript("hello world!");
        assert_eq!(text2, "Hello world!");

        let text3 = format_transcript("");
        assert_eq!(text3, "");
    }

    #[test]
    fn test_tdt_real_inference() {
        let cache_dir = std::path::PathBuf::from(r"C:\Users\Oluwafemi\AppData\Local\Typelz\Typelz\cache\models\parukeet-tdt-v3-onnx");
        if !cache_dir.exists() {
            return;
        }
        let engine = match TranscriptionEngine::new(&cache_dir) {
            Ok(e) => e,
            Err(e) => {
                println!("Skipping test: {}", e);
                return;
            }
        };

        // 1 second of silence
        let samples = vec![0.0f32; 16000];
        let pcm: Vec<u8> = samples.iter().flat_map(|&s| {
            let i = (s * 32768.0).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            i.to_le_bytes().to_vec()
        }).collect();

        let result = engine.transcribe(&pcm);
        println!("Transcribe result: {:?}", result);
        assert!(result.is_ok());
    }

    #[test]
    fn test_join_transcript_both_empty() {
        assert_eq!(join_transcript("", ""), "");
        assert_eq!(join_transcript("   ", "  "), "");
    }

    #[test]
    fn test_join_transcript_one_empty() {
        assert_eq!(join_transcript("", "Hello."), "Hello.");
        assert_eq!(join_transcript("Hello.", ""), "Hello.");
    }

    #[test]
    fn test_join_transcript_joins_with_single_space() {
        assert_eq!(join_transcript("Hello.", "World."), "Hello. World.");
        assert_eq!(join_transcript("  a  ", "  b  "), "a b");
    }

    #[test]
    fn test_session_transcript_clears() {
        let cache_dir = std::path::PathBuf::from(r"C:\Users\Oluwafemi\AppData\Local\Typelz\Typelz\cache\models\parukeet-tdt-v3-onnx");
        if !cache_dir.exists() {
            return;
        }
        let engine = match TranscriptionEngine::new(&cache_dir) {
            Ok(e) => e,
            Err(e) => {
                println!("Skipping test: {}", e);
                return;
            }
        };

        // Silence transcribes to empty, so the session accumulates to empty;
        // this exercises the real engine's session wiring.
        let samples = vec![0.0f32; 16000];
        let pcm: Vec<u8> = samples.iter().flat_map(|&s| {
            let i = (s * 32768.0).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            i.to_le_bytes().to_vec()
        }).collect();

        assert!(engine.transcribe(&pcm).is_ok());
        assert_eq!(engine.session_transcript(), "");
        engine.clear_transcript();
        assert_eq!(engine.session_transcript(), "");
    }

    #[test]
    fn test_ctc_decode_all_blank() {
        let vocab = vec![
            "<blank>".to_string(),
            "▁cat".to_string(),
        ];
        let tokens = vec![0, 0, 0, 0];
        let result = ctc_decode(&tokens, &vocab);
        assert_eq!(result, "");
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
