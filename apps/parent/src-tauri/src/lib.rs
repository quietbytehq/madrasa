//! The Madrasa parent app's shell: a Tauri 2 window around the web front end in
//! `apps/parent/src`. It is built for Android (`tauri android build`); the same code runs
//! as a small desktop window for development.
//!
//! The shell does nothing but show the front end. Everything the app knows comes from the
//! Nexus hub over HTTPS, asked for by the front end with the parent's own session
//! (docs/adr/0006-parent-app.md); there are no commands here for the page to call, no
//! local database and no school data stored on the phone.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running the Madrasa parent app");
}
