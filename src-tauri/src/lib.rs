mod audio;
mod model;
mod providers;
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
///
/// Async so the device validation and the (potentially slow) stop-join of a
/// previous capture run off the UI thread.
#[tauri::command]
async fn start_capture(
    app: tauri::AppHandle,
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let index: usize = device_id.parse().map_err(|_| format!("Invalid device ID: {device_id}"))?;

    // Take the previous capture handles out before blocking so stop+join run
    // on the worker thread, not the UI thread.
    let old_stop_tx = state.capture_stop_tx.lock().unwrap().take();
    let old_handle = state.capture_handle.lock().unwrap().take();
    let engine = state.transcription_engine.clone();
    let app_for_block = app.clone();

    let new_capture = tauri::async_runtime::spawn_blocking(move || {
        // Validate that the device exists before proceeding.
        let host = cpal::default_host();
        let devices = host.input_devices()
            .map_err(|e| format!("Failed to list input devices: {e}"))?;
        if !devices.enumerate().any(|(i, _)| i == index) {
            return Err(format!("Microphone device not found at index: {index}"));
        }

        // Signal any running capture to stop and wait for it to exit before
        // opening the next stream: the thread only exits after the signal, so
        // the signal must go out before the join.
        if let Some(tx) = old_stop_tx {
            audio::stop_capture(tx);
        }
        if let Some(handle) = old_handle {
            handle.join().unwrap_or_default();
        }

        audio::start_capture(app_for_block, index, engine)
    })
    .await
    .map_err(|e| format!("Start capture task failed: {e}"))??;

    state.capture_stop_tx.lock().unwrap().replace(new_capture.0);
    state.capture_handle.lock().unwrap().replace(new_capture.1);

    // A new recording begins with an already-loaded engine: start from an
    // empty session so this capture's Stop transcript cannot include the
    // previous recording.
    if let Some(eng) = state.transcription_engine.lock().unwrap().as_ref() {
        eng.clear_transcript();
    }

    Ok(())
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
///
/// Async so the capture-thread join runs off the UI thread; a slow in-flight
/// inference during the final flush must not freeze the window. Once the
/// capture thread exits, a non-empty session transcript is emitted so stopping
/// from the microphone controls also delivers the recording's transcript.
#[tauri::command]
async fn stop_capture(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    use tauri::Emitter;
    let stop_tx = state.capture_stop_tx.lock().unwrap().take();
    let capture_handle = state.capture_handle.lock().unwrap().take();
    let engine = state.transcription_engine.clone();

    tauri::async_runtime::spawn_blocking(move || {
        if let Some(tx) = stop_tx {
            audio::stop_capture(tx);
        }
        if let Some(handle) = capture_handle {
            handle.join().unwrap_or_default();
        }
        if let Some(eng) = engine.lock().unwrap().as_ref() {
            let session = eng.session_transcript();
            if !session.is_empty() {
                let _ = app.emit(
                    "transcription-result",
                    audio::TranscriptionResult { transcript: session },
                );
            }
        }
    })
    .await
    .map_err(|e| format!("Stop capture task failed: {e}"))?;
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

    let path_for_block = model_path.clone();
    let app_for_progress = app.clone();
    let engine_slot = state.transcription_engine.clone();

    // Model preparation and engine load share one blocking task so the
    // `ready` event below cannot fire until the engine actually exists.
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let status = model::ensure_on_path(
            &path_for_block,
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
        .map_err(|e| format!("Model preparation failed: {e}"))?;

        if !matches!(status, model::ModelStatus::Ready { .. }) {
            return Err(format!("Model cache not ready: {status:?}"));
        }

        let mut engine = engine_slot.lock().unwrap();
        if engine.is_none() {
            *engine = Some(
                transcribe::TranscriptionEngine::new(&path_for_block).map_err(|e| e.to_string())?,
            );
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("Model preparation task failed: {e}"))?;
    prepared?;

    // The engine is loaded by now, so the frontend can start capturing
    // without dropping early utterances.
    let _ = app.emit(
        "model-status",
        model::ModelStatusEvent {
            status: "ready".to_string(),
            file: None,
            progress: None,
        },
    );

    // New session: clear the previous recording's result so it cannot be
    // mistaken for the upcoming capture's transcript.
    if let Some(eng) = state.transcription_engine.lock().unwrap().as_ref() {
        eng.clear_transcript();
    }

    Ok(())
}

/// Stop capture (flushing any final utterance) and return the transcript of
/// the whole session — every utterance recorded since the last session clear,
/// not just the final fragment.
///
/// This command is async so that the blocking capture-thread join and any
/// in-flight inference run on a worker thread instead of freezing the UI.
#[tauri::command]
async fn transcribe_stop(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let stop_tx = state.capture_stop_tx.lock().unwrap().take();
    let capture_handle = state.capture_handle.lock().unwrap().take();
    let engine = state.transcription_engine.clone();

    tauri::async_runtime::spawn_blocking(move || {
        if let Some(tx) = stop_tx {
            audio::stop_capture(tx);
        }
        if let Some(handle) = capture_handle {
            handle.join().unwrap_or_default();
        }
        match engine.lock().unwrap().as_ref() {
            Some(eng) => Ok(eng.session_transcript()),
            None => Err("Transcription engine not initialized".to_string()),
        }
    })
    .await
    .map_err(|e| format!("Transcribe stop task failed: {e}"))?
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
    .manage(Arc::new(providers::KeyringCredentialStore) as Arc<dyn providers::CredentialStore>)
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
        providers::list_providers,
        providers::add_provider,
        providers::update_provider,
        providers::remove_provider,
        providers::validate_provider,
        providers::test_provider_credentials,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
