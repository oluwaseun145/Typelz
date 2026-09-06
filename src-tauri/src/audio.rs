use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

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

/// Audio frame event payload.
#[derive(serde::Serialize, Clone)]
pub struct AudioFrame {
    pub data: String, // base64-encoded PCM bytes
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

/// Start audio capture on the specified device (by index ID).
/// Spawns a background thread that owns the cpal Stream and forwards frames.
pub fn start_capture(
    app: tauri::AppHandle,
    device_index: usize,
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

    // Channel for signaling the background thread to stop.
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    // Bounded frame queue: bounded by max_frames, oldest frames dropped when full.
    const MAX_FRAMES: usize = 8;
    let frame_queue: Arc<Mutex<VecDeque<Vec<u8>>>> = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_FRAMES)));

    // Clone app handle once for use in all closures (AppHandle: Clone).
    let app_for_callback = app.clone();
    let app_for_post = app.clone();
    let app_for_loop = app.clone();

    let handle = std::thread::spawn(move || {
        use cpal::traits::DeviceTrait;

        let sample_format = config.sample_format();
        let queue_clone = frame_queue.clone();

        let stream_result = device.build_input_stream_raw(
            &config.into(),
            sample_format,
            move |data, _info| {
                let bytes: Vec<u8> = match data.as_slice::<f32>() {
                    Some(slice) => slice.iter()
                        .flat_map(|&s| s.to_le_bytes().to_vec())
                        .collect(),
                    None => match data.as_slice::<i16>() {
                        Some(slice) => slice.iter()
                            .flat_map(|&s| s.to_le_bytes().to_vec())
                            .collect(),
                        None => match data.as_slice::<i32>() {
                            Some(slice) => slice.iter()
                                .flat_map(|&s| s.to_le_bytes().to_vec())
                                .collect(),
                            None => vec![],
                        },
                    },
                };
                // Push to bounded queue, dropping oldest frames when full
                // to prevent unbounded memory growth.
                let mut q = queue_clone.lock().unwrap();
                // Drop oldest frames until within capacity to prevent unbounded growth.
                while q.len() >= MAX_FRAMES {
                    q.pop_front();
                }
                q.push_back(bytes);
            },
            move |err| {
                eprintln!("Audio capture error: {}", err);
                let _ = app_for_callback.emit("microphone-error", MicrophoneErrorEvent {
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

        // Drain the bounded queue as Tauri events, polling for the stop signal.
        loop {
            let frames: VecDeque<Vec<u8>> = {
                let mut q = frame_queue.lock().unwrap();
                let mut empty = VecDeque::with_capacity(MAX_FRAMES);
                std::mem::swap(&mut *q, &mut empty);
                empty
            };
            for bytes in frames {
                let _ = app_for_loop.emit("audio-frame", AudioFrame {
                    data: STANDARD.encode(&bytes),
                });
            }
            // Check for stop signal (non-blocking).
            if stop_rx.try_recv().is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Stream is dropped here, stopping capture.
    });

    Ok((stop_tx, handle))
}

/// Stop audio capture by signaling the background thread.
pub fn stop_capture(stop_tx: mpsc::Sender<()>) {
    let _ = stop_tx.send(()); // Signal the background thread to stop.
}


