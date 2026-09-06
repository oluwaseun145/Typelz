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

pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![get_tauri_info])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
