mod audio;
mod llm;
mod model;
mod providers;
mod transcribe;
mod vad;

use std::sync::{mpsc, Arc, Mutex};

use cpal::traits::HostTrait;
use tauri::Manager;

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

/// The single, authoritative ownership state of the capture pipeline.
///
/// Every start and stop command claims this slot for its whole in-flight
/// window. While claimed, no other start or stop can run, so the handles of a
/// live stream are always either handed to the next start or consumed by a
/// stop: a stream can never be orphaned into unstopability, and a stop can
/// never be a silent no-op against an in-flight start.
enum CaptureSlot {
    /// No capture and nothing in flight.
    Idle,
    /// A start or stop command owns the slot and is working on it.
    InFlight,
    /// A live capture stream; this is the only copy of its stop handles.
    Running {
        stop_tx: mpsc::Sender<()>,
        handle: std::thread::JoinHandle<()>,
    },
}

/// Claims the capture slot, returning its previous content to stop.
/// Fails loudly while another command holds the claim instead of interleaving
/// with it.
fn claim_capture_slot(slot: &mut CaptureSlot) -> Result<Option<CaptureSlot>, &'static str> {
    match &*slot {
        CaptureSlot::InFlight => Err("Capture is starting or stopping. Try again in a moment."),
        _ => Ok(Some(std::mem::replace(slot, CaptureSlot::InFlight))),
    }
}

/// Releases a finished claim. Only an InFlight slot is reset; anything else is
/// left untouched (defensive: the claim is exclusive while held).
fn release_capture_claim(slot: &mut CaptureSlot) {
    if matches!(*slot, CaptureSlot::InFlight) {
        *slot = CaptureSlot::Idle;
    }
}

/// Why a start failed, and what the slot must become as a result.
enum StartFailure {
    /// Device validation failed before the previous capture was stopped; the
    /// previous slot content must be restored so the live capture keeps its
    /// handles.
    Validate(String, CaptureSlot),
    /// The failure happened after the previous capture was already stopped;
    /// the claim simply returns to Idle.
    Post(String),
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

    // Claim before any work begins so the whole start (validate, stop old,
    // open new) is atomic with respect to other start/stop commands.
    let mut old = match claim_capture_slot(&mut state.capture_slot.lock().unwrap()) {
        Ok(prev) => prev,
        Err(msg) => return Err(msg.to_string()),
    };

    let engine = state.transcription_engine.clone();
    let app_for_block = app.clone();

    let outcome = tauri::async_runtime::spawn_blocking(move || {
        // Validate the device before touching any running capture, so a bad
        // device cannot take down a live recording.
        let host = cpal::default_host();
        let devices = host.input_devices()
            .map_err(|e| StartFailure::Validate(
                format!("Failed to list input devices: {e}"),
                old.take().unwrap_or(CaptureSlot::Idle),
            ))?;
        if !devices.enumerate().any(|(i, _)| i == index) {
            return Err(StartFailure::Validate(
                format!("Microphone device not found at index: {index}"),
                old.take().unwrap_or(CaptureSlot::Idle),
            ));
        }

        // Signal any running capture to stop and wait for it to exit before
        // opening the next stream: the thread only exits after the signal, so
        // the signal must go out before the join.
        if let CaptureSlot::Running { stop_tx, handle } = old.take().unwrap_or(CaptureSlot::Idle) {
            audio::stop_capture(stop_tx);
            handle.join().unwrap_or_default();
        }

        audio::start_capture(app_for_block, index, engine)
            .map_err(StartFailure::Post)
    })
    .await
    .map_err(|e| StartFailure::Post(format!("Start capture task failed: {e}")))
    .and_then(|r| r);

    // Publish or restore the slot. It is still InFlight (ours): no other
    // command could have changed it while the claim was held.
    let mut slot = state.capture_slot.lock().unwrap();
    match outcome {
        Ok((stop_tx, handle)) => {
            *slot = CaptureSlot::Running { stop_tx, handle };
        }
        Err(StartFailure::Validate(msg, previous)) => {
            *slot = previous;
            return Err(msg);
        }
        Err(StartFailure::Post(msg)) => {
            release_capture_claim(&mut slot);
            return Err(msg);
        }
    }

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
/// A stop that lands while a start is in flight is rejected with an
/// actionable error instead of silently no-oping.
#[tauri::command]
async fn stop_capture(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    use tauri::Emitter;
    let previous = match claim_capture_slot(&mut state.capture_slot.lock().unwrap()) {
        Ok(prev) => prev,
        Err(msg) => return Err(msg.to_string()),
    };
    let (stop_tx, handle) = match previous {
        Some(CaptureSlot::Running { stop_tx, handle }) => (stop_tx, handle),
        _ => {
            // The slot was Idle: nothing to stop.
            release_capture_claim(&mut state.capture_slot.lock().unwrap());
            return Ok(());
        }
    };
    let engine = state.transcription_engine.clone();

    let task = tauri::async_runtime::spawn_blocking(move || {
        audio::stop_capture(stop_tx);
        handle.join().unwrap_or_default();
        if let Some(eng) = engine.lock().unwrap().as_ref() {
            let session = eng.session_transcript();
            if !session.is_empty() {
                let _ = app.emit(
                    "transcription-result",
                    audio::TranscriptionResult { transcript: session },
                );
            }
        }
    });
    let task_result = task
        .await
        .map_err(|e| format!("Stop capture task failed: {e}"));
    release_capture_claim(&mut state.capture_slot.lock().unwrap());
    task_result?;
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
/// A stop that lands while a start is in flight is rejected with an
/// actionable error instead of silently no-oping.
#[tauri::command]
async fn transcribe_stop(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let previous = match claim_capture_slot(&mut state.capture_slot.lock().unwrap()) {
        Ok(prev) => prev,
        Err(msg) => return Err(msg.to_string()),
    };
    let engine = state.transcription_engine.clone();
    let (stop_tx, handle) = match previous {
        Some(CaptureSlot::Running { stop_tx, handle }) => (stop_tx, handle),
        _ => {
            // No live capture to stop. Return the accumulated session as-is
            // so a repeated stop cannot clear the user's last transcript.
            release_capture_claim(&mut state.capture_slot.lock().unwrap());
            return match engine.lock().unwrap().as_ref() {
                Some(eng) => Ok(eng.session_transcript()),
                None => Err("Transcription engine not initialized".to_string()),
            };
        }
    };

    let task = tauri::async_runtime::spawn_blocking(move || {
        audio::stop_capture(stop_tx);
        handle.join().unwrap_or_default();
        match engine.lock().unwrap().as_ref() {
            Some(eng) => Ok(eng.session_transcript()),
            None => Err("Transcription engine not initialized".to_string()),
        }
    });
    let task_result = task
        .await
        .map_err(|e| format!("Transcribe stop task failed: {e}"))
        .and_then(|r| r);
    release_capture_claim(&mut state.capture_slot.lock().unwrap());
    task_result
}

struct AppState {
    capture_slot: Mutex<CaptureSlot>,
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
        capture_slot: Mutex::new(CaptureSlot::Idle),
        model_cache: Mutex::new(None),
        transcription_engine: Arc::new(Mutex::new(None)),
    })
    .setup(|app| {
      let fallback = providers::FallbackCredentialStore::new(app.handle())
        .expect("Could not initialize the credential store");
      app.manage(Arc::new(fallback) as Arc<dyn providers::CredentialStore>);
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
        llm::send_chat_completion,
        llm::list_provider_models,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running_slot() -> CaptureSlot {
        let (stop_tx, _rx) = mpsc::channel::<()>();
        let handle = std::thread::spawn(|| {});
        CaptureSlot::Running { stop_tx, handle }
    }

    #[test]
    fn test_claim_idle_returns_previous_and_occupies() {
        let mut slot = CaptureSlot::Idle;
        let previous = claim_capture_slot(&mut slot).unwrap();
        assert!(matches!(previous, Some(CaptureSlot::Idle)));
        assert!(matches!(slot, CaptureSlot::InFlight));
    }

    #[test]
    fn test_claim_running_returns_previous_handles_and_occupies() {
        let mut slot = running_slot();
        let previous = claim_capture_slot(&mut slot).unwrap().unwrap();
        assert!(matches!(previous, CaptureSlot::Running { .. }));
        assert!(matches!(slot, CaptureSlot::InFlight));
    }

    #[test]
    fn test_claim_in_flight_is_rejected() {
        let mut slot = CaptureSlot::InFlight;
        let msg = match claim_capture_slot(&mut slot) {
            Err(msg) => msg,
            Ok(_) => panic!("claiming an InFlight slot must fail"),
        };
        assert!(msg.contains("Try again"));
        assert!(matches!(slot, CaptureSlot::InFlight));
    }

    #[test]
    fn test_release_restores_idle_only_from_in_flight() {
        let mut slot = CaptureSlot::InFlight;
        release_capture_claim(&mut slot);
        assert!(matches!(slot, CaptureSlot::Idle));

        // Never clobbers a published capture.
        let mut slot2 = running_slot();
        release_capture_claim(&mut slot2);
        assert!(matches!(slot2, CaptureSlot::Running { .. }));
    }
}
