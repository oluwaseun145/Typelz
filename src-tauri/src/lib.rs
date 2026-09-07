mod audio;
mod model;
mod transcribe;
mod vad;

use std::sync::{mpsc, Arc, Mutex};

use cpal::traits::HostTrait;

#[cfg_attr(mobile, tauri::mobile_entry_point)]

#[derive(serde::Serialize)]
struct TauriInfo {
    version: String,
    platform: String,
}

#[tauri::command]
fn get_tauri_info() -> TauriInfo {
    TauriInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

/// List available microphone devices.
#[tauri::command]
fn list_microphone_devices() -> Vec<audio::MicrophoneDevice> {
    audio::list_microphone_devices()
}

/// Start capture on a specific device (by index ID from the device list).
#[tauri::command]
fn start_capture(
    app: tauri::AppHandle,
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let index: usize = device_id.parse().map_err(|_| format!("Invalid device ID: {device_id}"))?;

    // Validate that the device exists before proceeding.
    let host = cpal::default_host();
    let devices = host.input_devices()
        .map_err(|e| format!("Failed to list input devices: {e}"))?;
    if !devices.enumerate().any(|(i, _)| i == index) {
        return Err(format!("Microphone device not found at index: {index}"));
    }

    stop_existing_capture(&state);

    let engine = state.transcription_engine.clone();
    let (stop_tx, handle) = audio::start_capture(app, index, engine)?;

    state.capture_stop_tx.lock().unwrap().replace(stop_tx);
    state.capture_handle.lock().unwrap().replace(handle);

    Ok(())
}

/// Signals the capture thread to stop and waits for it to exit. The stop
/// signal must go out before the join: the thread only exits after receiving
/// it, so joining first deadlocks when a capture is already running.
fn stop_existing_capture(state: &AppState) {
    if let Some(tx) = state.capture_stop_tx.lock().unwrap().take() {
        audio::stop_capture(tx);
    }
    if let Some(handle) = state.capture_handle.lock().unwrap().take() {
        handle.join().unwrap_or_default();
    }
}

/// Check the status of the Parukeet model cache.
#[tauri::command]
async fn get_model_status(state: tauri::State<'_, AppState>) -> Result<model::ModelStatus, String> {
    let cache = state.model_cache.lock().unwrap();
    match &*cache {
        Some(mc) => Ok(mc.status()),
        None => {
            // Create a temporary cache to check status
            match model::ModelCache::new() {
                Ok(mc) => Ok(mc.status()),
                Err(e) => Ok(model::ModelStatus::Error { message: e }),
            }
        }
    }
}



/// Stop capture. Idempotent.
#[tauri::command]
fn stop_capture(state: tauri::State<'_, AppState>) -> Result<(), String> {
    stop_existing_capture(&state);
    Ok(())
}

/// Start transcription mode: ensure models are ready, then load the engine.
///
/// The (potentially multi-GB) download runs on a blocking thread via
/// `spawn_blocking` and is awaited before the engine loads (F-18): the old
/// fire-and-forget thread raced the synchronous `ensure_on_path` check and the
/// engine load. While downloading, `model-status` events with byte progress
/// are emitted so the frontend can show progress.
#[tauri::command]
async fn transcribe_start(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    use tauri::Emitter;

    let model_path = {
        let cache = state.model_cache.lock().unwrap();
        match &*cache {
            Some(mc) => mc.path().to_path_buf(),
            None => {
                drop(cache);
                let mc = model::ModelCache::new()?;
                let path = mc.path().to_path_buf();
                state.model_cache.lock().unwrap().replace(mc);
                path
            }
        }
    };

    let path_for_ensure = model_path.clone();
    let app_for_progress = app.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        model::ensure_on_path(
            &path_for_ensure,
            Some(&|file: &str, downloaded: u64, total: Option<u64>| {
                let _ = app_for_progress.emit(
                    "model-status",
                    model::ModelStatusEvent {
                        status: "downloading".to_string(),
                        file: Some(file.to_string()),
                        progress: Some(model::DownloadProgress {
                            downloaded,
                            total: total.unwrap_or(0),
                        }),
                    },
                );
            }),
        )
    })
    .await
    .map_err(|e| format!("Model preparation task failed: {e}"))?
    .map_err(|e| format!("Model preparation failed: {e}"))?;

    if !matches!(status, model::ModelStatus::Ready { .. }) {
        return Err(format!("Model cache not ready: {status:?}"));
    }
    let _ = app.emit(
        "model-status",
        model::ModelStatusEvent {
            status: "ready".to_string(),
            file: None,
            progress: None,
        },
    );

    let mut engine = state.transcription_engine.lock().unwrap();
    if engine.is_none() {
        *engine = Some(
            transcribe::TranscriptionEngine::new(&model_path).map_err(|e| e.to_string())?,
        );
    }

    Ok(())
}

/// Stop capture (flushing any final utterance) and return the latest
/// transcript produced by the engine.
#[tauri::command]
fn transcribe_stop(state: tauri::State<'_, AppState>) -> Result<String, String> {
    stop_existing_capture(&state);
    match state.transcription_engine.lock().unwrap().as_ref() {
        Some(eng) => Ok(eng.last_transcript().unwrap_or_default()),
        None => Err("Transcription engine not initialized".to_string()),
    }
}

struct AppState {
    capture_stop_tx: Mutex<Option<mpsc::Sender<()>>>,
    capture_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    model_cache: Mutex<Option<model::ModelCache>>,
    /// Shared with running captures so utterances are transcribed as soon as
    /// `transcribe_start` loads the engine.
    transcription_engine: Arc<Mutex<Option<transcribe::TranscriptionEngine>>>,
}

pub fn run() {
  tauri::Builder::default()
    .plugin(
      tauri_plugin_log::Builder::default()
        .level(log::LevelFilter::Info)
        .build(),
    )
    .manage(AppState {
        capture_stop_tx: Mutex::new(None),
        capture_handle: Mutex::new(None),
        model_cache: Mutex::new(None),
        transcription_engine: Arc::new(Mutex::new(None)),
    })
    .setup(|_app| {
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
        get_tauri_info,
        list_microphone_devices,
        get_model_status,
        transcribe_start,
        transcribe_stop,
        start_capture,
        stop_capture,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
