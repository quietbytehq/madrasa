//! Madrasa desktop shell. Hosts the React UI and runs the API in-process (Lite): the UI never
//! talks to the database directly, it calls the local API over loopback.

mod backend;

use backend::Backend;
use tauri::Manager;

/// Returns the API base URL (e.g. `http://127.0.0.1:43123`) once the backend is up, or a
/// plain-language error if it failed to start.
#[tauri::command]
async fn api_base_url(backend: tauri::State<'_, Backend>) -> Result<String, String> {
    backend.base_url().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(Backend::start());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(backend) = window.app_handle().try_state::<Backend>() {
                    backend.shutdown();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![api_base_url])
        .run(tauri::generate_context!())
        .expect("error while running Madrasa");
}
