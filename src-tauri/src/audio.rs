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

/// Atomically detaches everything queued since the last drain.
fn drain(queue: &Mutex<VecDeque<Vec<u8>>>) -> VecDeque<Vec<u8>> {
    let mut q = queue.lock().unwrap();
    let mut empty = VecDeque::new();
    std::mem::swap(&mut *q, &mut empty);
    empty
}

/// Emits `utterance-ready`, then routes the utterance through the
/// transcription engine when one is loaded, emitting `transcription-result`
/// (or `microphone-error` on inference failure). Runs on the forwarding
/// thread, not the real-time audio callback (F-21, F-23).
fn handle_utterance(
    app: &tauri::AppHandle,
    engine: &Mutex<Option<TranscriptionEngine>>,
    pcm: &[u8],
) {
    use tauri::Emitter;

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
        Ok(transcript) => {
            let _ = app.emit("transcription-result", TranscriptionResult { transcript });
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
                let f32_samples: Vec<f32> = match data.as_slice::<f32>() {
                    Some(slice) => slice.to_vec(),
                    None => match data.as_slice::<i16>() {
                        Some(slice) => slice.iter().map(|&s| s as f32 / 32768.0).collect(),
                        None => match data.as_slice::<i32>() {
                            Some(slice) => slice.iter().map(|&s| s as f32 / 2147483648.0).collect(),
                            None => vec![],
                        },
                    },
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
                    return;
                }

                match state {
                    VadState::Speeching => {
                        let mut buf = buffer_clone.lock().unwrap();
                        buf.extend_from_slice(&resampled);
                    }
                    VadState::Boundary => {
                        let buf = buffer_clone.lock().unwrap();
                        if !buf.is_empty() {
                            let mut q = utterance_queue_clone.lock().unwrap();
                            while q.len() >= MAX_UTTERANCES {
                                q.pop_front();
                            }
                            q.push_back(f32_to_i16_le(&buf));
                        }
                        drop(buf);
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

        loop {
            let frames = drain(&frame_queue);
            let utterances = drain(&utterance_queue);

            for bytes in frames {
                let _ = app_for_loop.emit("audio-frame", AudioFrame {
                    data: STANDARD.encode(&bytes),
                });
            }
            for pcm in &utterances {
                handle_utterance(&app_for_loop, &engine, &pcm);
            }

            let stop_requested = stop_rx.try_recv().is_ok();
            if stop_requested && utterances.is_empty() {
                break;
            }
            if !stop_requested {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        // A trailing callback can land between the final drain and the stream
        // drop; process it so stopping returns the last transcript too.
        for pcm in drain(&utterance_queue) {
            handle_utterance(&app_for_loop, &engine, &pcm);
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
}
