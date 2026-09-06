mod audio;

use std::sync::mpsc;

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
    let index: usize = device_id.parse().map_err(|_| format!("Invalid device ID: {}", device_id))?;

    // Stop any existing capture first (idempotent restart).
    {
        let mut handle_guard = state.capture_handle.lock().unwrap();
        if let Some(handle) = handle_guard.take() {
            handle.join().unwrap_or_default();
        }
    }
    {
        let mut tx_guard = state.capture_stop_tx.lock().unwrap();
        if let Some(tx) = tx_guard.take() {
            audio::stop_capture(tx);
        }
    }

    let (stop_tx, handle) = audio::start_capture(app, index)?;

    {
        let mut tx_guard = state.capture_stop_tx.lock().unwrap();
        *tx_guard = Some(stop_tx);
    }
    {
        let mut handle_guard = state.capture_handle.lock().unwrap();
        *handle_guard = Some(handle);
    }

    Ok(())
}

/// Stop capture. Idempotent.
#[tauri::command]
fn stop_capture(state: tauri::State<'_, AppState>) -> Result<(), String> {
    // Signal the background thread to stop.
    {
        let mut tx_guard = state.capture_stop_tx.lock().unwrap();
        if let Some(tx) = tx_guard.take() {
            audio::stop_capture(tx);
        }
    }
    // Wait for the thread to finish to prevent concurrent capture threads.
    {
        let mut handle_guard = state.capture_handle.lock().unwrap();
        if let Some(handle) = handle_guard.take() {
            handle.join().unwrap_or_default();
        }
    }
    Ok(())
}

struct AppState {
    capture_stop_tx: std::sync::Mutex<Option<mpsc::Sender<()>>>,
    capture_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

pub fn run() {
  tauri::Builder::default()
    .plugin(
      tauri_plugin_log::Builder::default()
        .level(log::LevelFilter::Info)
        .build(),
    )
    .manage(AppState {
        capture_stop_tx: std::sync::Mutex::new(None),
        capture_handle: std::sync::Mutex::new(None),
    })
    .setup(|_app| {
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
        get_tauri_info,
        list_microphone_devices,
        start_capture,
        stop_capture,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
