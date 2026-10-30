//! Madrasa desktop shell. Hosts the React UI and (from task 5 on) the in-process API.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running Madrasa");
}
