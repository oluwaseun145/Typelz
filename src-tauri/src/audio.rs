use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::transcribe::TranscriptionEngine;
use crate::vad::{VadDetector, VadState};

/// Represents an available microphone device.
#[derive(serde::Serialize, Clone)]
pub struct MicrophoneDevice {
    pub id: String,
    pub name: String,
}

/// Error event sent to the frontend.
#[derive(serde::Serialize, Clone)]
pub struct MicrophoneErrorEvent {
    pub message: String,
}

/// Audio frame event payload (raw PCM frames for backward compatibility).
#[derive(serde::Serialize, Clone)]
pub struct AudioFrame {
    pub data: String, // base64-encoded PCM bytes
}

/// Utterance-ready event payload (complete speech segment with VAD boundaries).
#[derive(serde::Serialize, Clone)]
pub struct UtteranceReadyEvent {
    pub data: String, // base64-encoded 16 kHz mono i16 LE PCM utterance
}

/// Transcription result event payload.
#[derive(serde::Serialize, Clone)]
pub struct TranscriptionResult {
    pub transcript: String,
}

/// List all available input (microphone) devices with index-based IDs.
pub fn list_microphone_devices() -> Vec<MicrophoneDevice> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices
            .enumerate()
            .map(|(i, device)| MicrophoneDevice {
                id: i.to_string(),
                name: device.name().unwrap_or_else(|_| format!("<unknown device {}>", i)),
            })
            .collect(),
        Err(e) => {
            eprintln!("Failed to list input devices: {}", e);
            vec![]
        }
    }
}

/// Resample audio using linear interpolation with anti-aliasing low-pass filter.
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }
    if samples.is_empty() || to_rate == 0 {
        return vec![];
    }

    // Apply a simple anti-aliasing low-pass filter before downsampling.
    // Cutoff at 0.45 * min(from_rate, to_rate) to avoid aliasing artifacts.
    let cutoff = if from_rate > to_rate {
        // Downsampling: need anti-aliasing
        (to_rate as f64 * 0.45).max(1.0)
    } else {
        // Upsampling: no aliasing risk, skip filter
        0.0
    };

    let filtered = if cutoff > 0.0 && from_rate > to_rate {
        low_pass_filter(samples, from_rate as f64, cutoff)
    } else {
        samples.to_vec()
    };

    let output_len = (filtered.len() as f64 * to_rate as f64 / from_rate as f64).ceil() as usize;
    if output_len == 0 {
        return vec![];
    }

    // Linear interpolation resampling.
    let mut result = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let src_pos = i * from_rate as usize / to_rate as usize;
        let src_frac = (i * from_rate as usize) % to_rate as usize;
        if src_frac == 0 || src_pos >= filtered.len() {
            result.push(filtered[src_pos]);
        } else {
            let next_pos = (src_pos + 1).min(filtered.len() - 1);
            let frac = src_frac as f32 / to_rate as f32;
            let a = filtered[src_pos];
            let b = filtered[next_pos];
            result.push(a + frac * (b - a));
        }
    }
    result
}

/// Simple moving-average low-pass filter for anti-aliasing.
fn low_pass_filter(samples: &[f32], sample_rate: f64, cutoff_freq: f64) -> Vec<f32> {
    if cutoff_freq <= 0.0 || samples.len() < 2 {
        return samples.to_vec();
    }
    // Kernel size inversely proportional to cutoff frequency.
    let kernel_size = (2.0 * sample_rate / cutoff_freq).max(3.0) as usize;
    // The moving average needs an odd kernel to stay symmetric around each sample.
    let kernel_size = if kernel_size % 2 == 0 {
        kernel_size.saturating_add(1)
    } else {
        kernel_size
    };

    let half = kernel_size / 2;
    let mut result = Vec::with_capacity(samples.len());
    for i in 0..samples.len() {
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for j in (if i >= half { i - half } else { 0 })..=(
            if i + half < samples.len() { i + half } else { samples.len() - 1 }
        ) {
            sum += samples[j];
            count += 1;
        }
        result.push(if count > 0 { sum / count as f32 } else { samples[i] });
    }
    result
}

/// Converts f32 samples (range -1..=1) to little-endian i16 PCM bytes.
fn f32_to_i16_le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.max(-1.0).min(1.0) * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Downmixes an interleaved multi-channel stream to mono by selecting the
/// loudest channel, instead of averaging all of them.
///
/// cpal delivers frames interleaved per channel (L, R, C, S, L, R, ...).
/// Treating that stream as mono time-stretches and scrambles the audio, so
/// multi-channel devices must be downmixed before VAD or inference. A device
/// such as the 4-channel Realtek mic carries the microphone in only one
/// channel; averaging all four attenuates the signal by ~12 dB, so the VAD
/// and model hear mostly noise. Picking the loudest channel keeps the real
/// signal at full amplitude. The selection is sticky (EMA of per-channel
/// energy plus a hysteresis gap) so it does not flip-flop between channels
/// on small fluctuations.
struct DominantChannelDownmix {
    /// Per-channel running energy estimate (EMA of mean-square amplitude).
    energy: Vec<f32>,
    /// Index of the currently selected (loudest) channel.
    selected: usize,
}

impl DominantChannelDownmix {
    fn new(channels: usize) -> Self {
        Self {
            energy: vec![0.0; channels],
            selected: 0,
        }
    }

    fn downmix(&mut self, interleaved: &[f32], channels: usize) -> Vec<f32> {
        if channels <= 1 {
            return interleaved.to_vec();
        }
        // Update the per-channel energy EMA from this buffer.
        let mut mean_sq = vec![0.0f32; channels];
        let mut count = vec![0usize; channels];
        for frame in interleaved.chunks_exact(channels) {
            for (ch, &sample) in frame.iter().enumerate() {
                mean_sq[ch] += sample * sample;
                count[ch] += 1;
            }
        }
        for ch in 0..channels {
            let rms_sq = if count[ch] > 0 {
                mean_sq[ch] / count[ch] as f32
            } else {
                0.0
            };
            self.energy[ch] = self.energy[ch] * 0.8 + rms_sq * 0.2;
        }
        // Switch only when another channel is clearly louder (hysteresis).
        for ch in 0..channels {
            if ch != self.selected && self.energy[ch] > self.energy[self.selected] * 2.0 {
                self.selected = ch;
            }
        }
        interleaved
            .chunks_exact(channels)
            .filter_map(|frame| frame.get(self.selected).copied())
            .collect()
    }
}

/// Atomically detaches everything queued since the last drain.
fn drain(queue: &Mutex<VecDeque<Vec<u8>>>) -> VecDeque<Vec<u8>> {
    let mut q = queue.lock().unwrap();
    let mut empty = VecDeque::new();
    std::mem::swap(&mut *q, &mut empty);
    empty
}

/// Drains the frame and utterance queues on the capture thread until the
/// one-shot stop message is latched and the queues are empty.
///
/// The stop message is a single mpsc value. It is latched on first delivery so
/// a utterance drained in the same iteration that consumes it cannot swallow
/// the request: without the latch the next `try_recv` sees an empty channel
/// and the loop captures audio forever.
fn run_forwarding_loop(
    stop_rx: mpsc::Receiver<()>,
    frame_queue: &Mutex<VecDeque<Vec<u8>>>,
    utterance_queue: &Mutex<VecDeque<Vec<u8>>>,
    mut process_frames: impl FnMut(Vec<u8>),
    mut process_utterances: impl FnMut(&[u8]),
) {
    let mut stop_latched = false;
    loop {
        let frames = drain(frame_queue);
        let utterances = drain(utterance_queue);

        for bytes in frames {
            process_frames(bytes);
        }
        for pcm in &utterances {
            process_utterances(pcm);
        }

        if !stop_latched && stop_rx.try_recv().is_ok() {
            stop_latched = true;
        }
        if stop_latched && utterances.is_empty() {
            break;
        }
        if !stop_latched {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// Minimum utterance length, in 16 kHz mono samples, before inference.
/// WebRTC VAD can fire on short noise blips (~0.1-0.3 s); those would be sent
/// to the model and can produce random fragments, so shorter segments are
/// skipped. 300 ms keeps real short phrases while dropping blips.
const MIN_UTTERANCE_SAMPLES_16K: usize = 4800;

/// Emits `utterance-ready`, then routes the utterance through the
/// transcription engine when one is loaded (skipping sub-300 ms noise blips).
/// The session transcript is delivered once at stop time, not per utterance.
/// Runs on the forwarding thread, not the real-time audio callback (F-21, F-23).
fn handle_utterance(
    app: &tauri::AppHandle,
    engine: &Mutex<Option<TranscriptionEngine>>,
    pcm: &[u8],
) {
    use tauri::Emitter;

    let samples = pcm.len() / 2; // i16 LE
    if samples < MIN_UTTERANCE_SAMPLES_16K {
        eprintln!("[typelz] skip: utterance too short (~{:.2}s)", samples as f64 / 16000.0);
        return;
    }

    let _ = app.emit(
        "utterance-ready",
        UtteranceReadyEvent {
            data: STANDARD.encode(pcm),
        },
    );

    let guard = engine.lock().unwrap();
    let Some(eng) = guard.as_ref() else {
        return;
    };
    match eng.transcribe(pcm) {
        Ok(_transcript) => {
            // Accumulated in the engine's session transcript; the stop
            // command/return value delivers the full recording's text.
        }
        Err(e) => {
            eprintln!("Transcription failed: {e}");
            let _ = app.emit(
                "microphone-error",
                MicrophoneErrorEvent {
                    message: format!("Transcription error: {e}"),
                },
            );
        }
    }
}

pub fn start_capture(
    app: tauri::AppHandle,
    device_index: usize,
    engine: Arc<Mutex<Option<TranscriptionEngine>>>,
) -> Result<(mpsc::Sender<()>, std::thread::JoinHandle<()>), String> {
    use tauri::Emitter;

    let host = cpal::default_host();

    // Find the target device by index.
    let device = match host.input_devices() {
        Ok(devices) => devices.enumerate().nth(device_index).map(|(_, d)| d),
        Err(e) => {
            eprintln!("Failed to list input devices: {}", e);
            None
        }
    }.ok_or_else(|| format!("Microphone device not found at index: {}", device_index))?;

    let config = device.default_input_config()
        .map_err(|e| format!("Failed to get default input config: {}", e))?;

    // The downstream pipeline (resampler, VAD, Parakeet) is mono-only, but a
    // device's default config can be multi-channel (e.g. the 4-channel
    // Realtek mic on this machine). Interleaved channels read as mono are
    // time-stretched and scrambled, so the callback downmixes when needed.
    let native_channels = config.channels();

    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    const MAX_FRAMES: usize = 8;
    let frame_queue: Arc<Mutex<VecDeque<Vec<u8>>>> = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_FRAMES)));
    const MAX_UTTERANCES: usize = 4;
    let utterance_queue: Arc<Mutex<VecDeque<Vec<u8>>>> = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_UTTERANCES)));
    let utterance_buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

    let app_for_callback = app.clone();
    let app_for_error_cb = app.clone();
    let app_for_post = app.clone();
    let app_for_loop = app.clone();
    // Single owner, moved into the stream callback that cpal invokes serially.
    let mut vad = Some(VadDetector::new());
    // Tracks the current speech run so the trace prints transitions only,
    // not every callback frame.
    let in_speech = Cell::new(false);
    // Picks the loudest channel of a multi-channel device instead of
    // averaging all of them (which would attenuate a single-channel mic).
    let mut downmix = DominantChannelDownmix::new(native_channels as usize);

    let handle = std::thread::spawn(move || {
        use cpal::traits::DeviceTrait;

        let sample_format = config.sample_format();
        let device_sample_rate = config.sample_rate().0;
        const TARGET_RATE: u32 = 16000;

        let queue_clone = frame_queue.clone();
        let utterance_queue_clone = utterance_queue.clone();
        let buffer_clone = utterance_buffer.clone();

        let stream_result = device.build_input_stream_raw(
            &config.into(),
            sample_format,
            move |data, _info| {
                let raw_samples: Vec<f32> = match data.as_slice::<f32>() {
                    Some(slice) => slice.to_vec(),
                    None => match data.as_slice::<i16>() {
                        Some(slice) => slice.iter().map(|&s| s as f32 / 32768.0).collect(),
                        None => match data.as_slice::<i32>() {
                            Some(slice) => slice.iter().map(|&s| s as f32 / 2147483648.0).collect(),
                            None => vec![],
                        },
                    },
                };
                // Multi-channel devices deliver interleaved frames; the
                // pipeline is mono-only, so downmix instead of feeding
                // scrambled channel data into the VAD and model. Pick the
                // loudest channel so a single-channel mic is not attenuated
                // by averaging in the silent channels.
                let f32_samples = if native_channels == 1 {
                    raw_samples
                } else {
                    downmix.downmix(&raw_samples, native_channels as usize)
                };

                let bytes: Vec<u8> = f32_samples.iter()
                    .flat_map(|&s| s.to_le_bytes().to_vec())
                    .collect();
                let mut q = queue_clone.lock().unwrap();
                while q.len() >= MAX_FRAMES {
                    q.pop_front();
                }
                q.push_back(bytes);

                let resampled = if device_sample_rate != TARGET_RATE {
                    resample(&f32_samples, device_sample_rate, TARGET_RATE)
                } else {
                    f32_samples.clone()
                };

                let Some(vad) = vad.as_mut() else { return; };
                let state = vad.process_frame(&resampled);

                if vad.is_error() {
                    let err_msg = vad.error().map(|e| e.to_string()).unwrap_or_default();
                    let _ = app_for_callback.emit("microphone-error", MicrophoneErrorEvent {
                        message: format!("VAD error: {err_msg}"),
                    });
                    // Clear the utterance buffer to prevent stale data.
                    if let Ok(mut buf) = buffer_clone.lock() {
                        buf.clear();
                    }
                    in_speech.set(false);
                    return;
                }

                match state {
                    VadState::Speeching => {
                        let mut buf = buffer_clone.lock().unwrap();
                        if !in_speech.get() {
                            in_speech.set(true);
                            eprintln!("[typelz] VAD: speech start");
                        }
                        buf.extend_from_slice(&resampled);
                    }
                    VadState::Boundary => {
                        let utterance_seconds = {
                            let mut buf = buffer_clone.lock().unwrap();
                            if !buf.is_empty() {
                                let seconds = buf.len() as f64 / 16000.0;
                                let mut q = utterance_queue_clone.lock().unwrap();
                                while q.len() >= MAX_UTTERANCES {
                                    q.pop_front();
                                }
                                q.push_back(f32_to_i16_le(&buf));
                                // Clear the buffer so the next utterance starts
                                // fresh; without this, every subsequent boundary
                                // re-queues the entire accumulated audio.
                                buf.clear();
                                Some(seconds)
                            } else {
                                None
                            }
                        };
                        if let Some(seconds) = utterance_seconds {
                            eprintln!("[typelz] VAD: utterance end (~{seconds:.2}s)");
                        }
                        in_speech.set(false);
                        vad.reset();
                    }
                    VadState::Silent => {}
                }
            },
            move |err| {
                eprintln!("Audio capture error: {}", err);
                let _ = app_for_error_cb.emit("microphone-error", MicrophoneErrorEvent {
                    message: format!("Audio error: {}", err),
                });
            },
            None,
        );

        let stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to build input stream: {}", e);
                let _ = app_for_post.emit("microphone-error", MicrophoneErrorEvent {
                    message: format!("Stream error: {}", e),
                });
                return;
            }
        };

        if let Err(e) = stream.play() {
            eprintln!("Failed to start stream: {}", e);
            let _ = app_for_post.emit("microphone-error", MicrophoneErrorEvent {
                message: format!("Play error: {}", e),
            });
            return;
        }

        eprintln!(
            "[typelz] capture started: {} Hz, native_channels={}, mode={}",
            device_sample_rate,
            native_channels,
            if native_channels == 1 { "device-mono" } else { "downmixed-mono" },
        );

        run_forwarding_loop(
            stop_rx,
            &frame_queue,
            &utterance_queue,
            |bytes| {
                let _ = app_for_loop.emit("audio-frame", AudioFrame {
                    data: STANDARD.encode(&bytes),
                });
            },
            |pcm| handle_utterance(&app_for_loop, &engine, pcm),
        );
        // A trailing callback can land between the final drain and the stream
        // drop; process it so stopping returns the last transcript too.
        for pcm in drain(&utterance_queue) {
            handle_utterance(&app_for_loop, &engine, &pcm);
        }

        // Flush any speech still sitting in the utterance buffer that never
        // reached a VAD Boundary (e.g. user was still speaking when Stop was
        // pressed). Without this, continuous speech with no pause is lost.
        {
            let buf = utterance_buffer.lock().unwrap();
            if !buf.is_empty() {
                let seconds = buf.len() as f64 / 16000.0;
                let pcm = f32_to_i16_le(&buf);
                drop(buf);
                eprintln!("[typelz] flush: final utterance (~{seconds:.2}s)");
                handle_utterance(&app_for_loop, &engine, &pcm);
            }
        }

    });

    Ok((stop_tx, handle))
}

pub fn stop_capture(stop_tx: mpsc::Sender<()>) {
    let _ = stop_tx.send(());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_same_rate() {
        let input: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let output = resample(&input, 16000, 16000);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn test_resample_empty() {
        let output = resample(&[], 48000, 16000);
        assert!(output.is_empty());
    }

    #[test]
    fn test_resample_downsample_ratio() {
        // 48 kHz → 16 kHz should give roughly 1/3 the samples.
        let input: Vec<f32> = (0..300).map(|i| (i as f32 * 0.1).sin()).collect();
        let output = resample(&input, 48000, 16000);
        let expected_len = (input.len() as f64 * 16000.0 / 48000.0).ceil() as usize;
        assert!((output.len() as i64 - expected_len as i64).abs() <= 1);
    }

    #[test]
    fn test_resample_upsample_ratio() {
        // 16 kHz → 48 kHz should give roughly 3x the samples.
        let input: Vec<f32> = (0..100).map(|i| (i as f32 * 0.1).sin()).collect();
        let output = resample(&input, 16000, 48000);
        let expected_len = (input.len() as f64 * 48000.0 / 16000.0).ceil() as usize;
        assert!((output.len() as i64 - expected_len as i64).abs() <= 1);
    }

    #[test]
    fn test_low_pass_filter_stabilizes() {
        // A simple low-pass filter should not amplify values beyond input range.
        let input: Vec<f32> = (0..50).map(|i| (i as f32 * 0.2 - 5.0).sin()).collect();
        let output = super::low_pass_filter(&input, 48000.0, 2000.0);
        assert_eq!(output.len(), input.len());
        for &v in &output {
            assert!(v.abs() <= 1.0 + 1e-6, "Filtered value out of range: {v}");
        }
    }

    #[test]
    fn test_f32_to_i16_le_values() {
        let pcm = f32_to_i16_le(&[0.0, 1.0, -1.0]);
        assert_eq!(pcm, vec![0x00, 0x00, 0xff, 0x7f, 0x01, 0x80]);
    }

    #[test]
    fn test_f32_to_i16_le_clamps_out_of_range() {
        let pcm = f32_to_i16_le(&[2.0, -3.0]);
        assert_eq!(pcm, vec![0xff, 0x7f, 0x01, 0x80]);
        assert_eq!(pcm.len(), 4);
    }

    #[test]
    fn test_dominant_channel_passthrough_single_channel() {
        let mut d = DominantChannelDownmix::new(1);
        let input = vec![0.25, -0.75, 1.0];
        assert_eq!(d.downmix(&input, 1), input);
    }

    #[test]
    fn test_dominant_channel_picks_loudest_channel() {
        // Channel 1 is much louder than channel 0, so it is selected and its
        // samples are passed through at full amplitude (not averaged down).
        let mut d = DominantChannelDownmix::new(2);
        let input: Vec<f32> = (0..8)
            .map(|i| if i % 2 == 0 { 0.01 } else { 0.9 })
            .collect();
        let out = d.downmix(&input, 2);
        assert_eq!(out, vec![0.9, 0.9, 0.9, 0.9]);
    }

    #[test]
    fn test_dominant_channel_four_channels_picks_loudest() {
        // The 4-channel case this machine's Realtek mic hits: only channel 2
        // carries the signal, so it is selected at full amplitude.
        let mut d = DominantChannelDownmix::new(4);
        let input: Vec<f32> = vec![0.0, 0.0, 0.8, 0.0, 0.0, 0.0, -0.8, 0.0];
        let out = d.downmix(&input, 4);
        assert_eq!(out, vec![0.8, -0.8]);
    }

    #[test]
    fn test_dominant_channel_drops_trailing_partial_frame() {
        // A trailing value that does not complete a channel frame is dropped.
        let mut d = DominantChannelDownmix::new(2);
        let out = d.downmix(&[1.0, 1.0, 0.5], 2);
        assert_eq!(out, vec![1.0]);
    }

    #[test]
    fn test_dominant_channel_hysteresis_stays_put() {
        // Once a channel is selected, a small fluctuation on another channel
        // must not flip the selection (hysteresis gap of 2x).
        let mut d = DominantChannelDownmix::new(2);
        // First buffer: channel 0 is clearly louder → selected.
        let first: Vec<f32> = (0..20).map(|i| if i % 2 == 0 { 1.0 } else { 0.1 }).collect();
        let _ = d.downmix(&first, 2);
        assert_eq!(d.selected, 0);
        // Second buffer: channel 1 is slightly louder but not 2x → stays on 0.
        let second: Vec<f32> = (0..20).map(|i| if i % 2 == 0 { 1.0 } else { 1.5 }).collect();
        let out = d.downmix(&second, 2);
        assert_eq!(d.selected, 0);
        assert_eq!(out, vec![1.0; 10]);
    }

    #[test]
    fn test_forwarding_loop_exits_on_stop_with_empty_queues() {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let frame_queue: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());
        let utterance_queue: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());
        stop_tx.send(()).unwrap();

        let (exit_tx, exit_rx) = mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            run_forwarding_loop(stop_rx, &frame_queue, &utterance_queue, |_| {}, |_| {});
            let _ = exit_tx.send(());
        });

        assert!(
            exit_rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok(),
            "loop did not exit after stop"
        );
        handle.join().expect("capture thread panicked");
    }

    #[test]
    fn test_forwarding_loop_stops_when_stop_consumed_with_queued_utterance() {
        // The exact failure the latch fixes: the stop message is consumed on
        // the same iteration that drains a queued utterance. Without the
        // latch the next try_recv sees an empty channel and the loop never
        // exits, so capture (and inference) runs forever after Stop.
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let frame_queue: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());
        let utterance_queue: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());
        utterance_queue.lock().unwrap().push_back(vec![1, 2, 3]);
        stop_tx.send(()).unwrap();

        let (utterance_tx, utterance_rx) = mpsc::channel::<Vec<u8>>();
        let (exit_tx, exit_rx) = mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            run_forwarding_loop(
                stop_rx,
                &frame_queue,
                &utterance_queue,
                |_| {},
                |pcm| utterance_tx.send(pcm.to_vec()).unwrap(),
            );
            let _ = exit_tx.send(());
        });

        assert_eq!(
            utterance_rx.recv_timeout(std::time::Duration::from_secs(5)).ok(),
            Some(vec![1, 2, 3]),
            "queued utterance was not processed"
        );
        assert!(
            exit_rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok(),
            "loop did not exit after stop: the stop signal was swallowed"
        );
        handle.join().expect("capture thread panicked");
    }
}
