mod adapters;
pub mod approvals;
pub mod control;
pub mod events;
pub mod registry;
mod tv;

pub use adapters::{Adapter, Session};
pub use registry::{scan_sessions, watch_sessions, watch_sessions_cli, Harness};

use control::adopt::adopt_session;
use events::{
    approve_session, deny_session, get_yolo, kill_session, list_sessions, send_prompt, set_yolo,
    spawn_session, start_watcher, AppState,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let owned_path = data_dir.join("owned.json");
            let owned_map = control::owned::load(&owned_path);
            let state = Arc::new(AppState {
                snapshot: std::sync::Mutex::new(Vec::new()),
                owned: std::sync::Mutex::new(owned_map),
                owned_path: std::sync::Mutex::new(owned_path),
                pending: std::sync::Mutex::new(HashMap::new()),
                approvals: std::sync::Mutex::new(HashMap::new()),
                yolo: std::sync::Mutex::new(false),
                yolo_seen: std::sync::Mutex::new(std::collections::HashSet::new()),
            });
            app.manage(Arc::clone(&state));
            start_watcher(app.handle().clone(), state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_sessions,
            spawn_session,
            send_prompt,
            kill_session,
            adopt_session,
            approve_session,
            deny_session,
            set_yolo,
            get_yolo,
            tv::toggle_tv,
            tv::tv_interrupt
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            control::opencode::shutdown();
        }
    });
}
