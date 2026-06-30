mod adapters;
pub mod registry;

pub use adapters::{Adapter, Session};
pub use registry::{scan_sessions, watch_sessions, Harness};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
