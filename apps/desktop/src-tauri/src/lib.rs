//! Madrasa desktop shell. Hosts the React UI and runs the API in-process (Lite): the UI never
//! talks to the database directly, it calls the local API over loopback. In LAN mode the same
//! API is also served to other PCs over TLS, or this PC proxies to a school server.

mod backend;
mod mode;

use backend::Backend;
use madrasa_api::bootstrap::default_data_dir;
use madrasa_api::lan;
use mode::{ModeConfig, NetworkInfo};
use std::sync::Mutex;
use tauri::Manager;

/// The backend once a mode is chosen; `None` on a fresh install until the user chooses.
struct Shell {
    backend: Mutex<Option<Backend>>,
}

impl Shell {
    fn ensure_started(&self, config: ModeConfig) {
        let mut guard = self.backend.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(Backend::start(config));
        }
    }
}

/// Returns the API base URL (e.g. `http://127.0.0.1:43123`) once the backend is up, a
/// plain-language error if it failed to start, or `mode_not_chosen` on a fresh install.
#[tauri::command]
async fn api_base_url(shell: tauri::State<'_, Shell>) -> Result<String, String> {
    let backend = {
        let guard = shell.backend.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|b| b.handle())
    };
    match backend {
        Some(b) => b.base_url().await,
        None => Err("mode_not_chosen".into()),
    }
}

/// The saved mode, or `None` on a fresh install.
#[tauri::command]
fn get_mode() -> Option<ModeConfig> {
    ModeConfig::load(&default_data_dir())
}

/// Save the mode and start the backend accordingly.
#[tauri::command]
fn set_mode(shell: tauri::State<'_, Shell>, config: ModeConfig) -> Result<ModeConfig, String> {
    config.validate().map_err(|e| e.to_string())?;
    config
        .save(&default_data_dir())
        .map_err(|e| format!("Could not save the mode: {e}"))?;
    shell.ensure_started(config.clone());
    Ok(config)
}

/// Addresses, port and fingerprint of this server (or the server a client is connected to).
#[tauri::command]
fn network_info(shell: tauri::State<'_, Shell>) -> Option<NetworkInfo> {
    let guard = shell.backend.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().and_then(|b| b.network_info())
}

/// Madrasa servers answering on the LAN (about two seconds).
#[tauri::command]
async fn discover_servers() -> Vec<lan::Found> {
    lan::discover(std::time::Duration::from_millis(1500)).await
}

/// Connect once to a server and report its certificate fingerprint and version.
#[tauri::command]
async fn probe_server(url: String) -> Result<lan::Probe, String> {
    lan::probe(&url).await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();
    lan::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let shell = Shell {
                backend: Mutex::new(None),
            };
            // An existing install keeps its mode; a fresh one waits for the user's choice.
            if let Some(config) = ModeConfig::load(&default_data_dir()) {
                shell.ensure_started(config);
            }
            app.manage(shell);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(shell) = window.app_handle().try_state::<Shell>() {
                    if let Some(backend) = shell
                        .backend
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_ref()
                    {
                        backend.shutdown();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            api_base_url,
            get_mode,
            set_mode,
            network_info,
            discover_servers,
            probe_server
        ])
        .run(tauri::generate_context!())
        .expect("error while running Madrasa");
}
