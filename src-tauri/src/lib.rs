mod adapters;
pub mod approvals;
pub mod control;
pub mod events;
pub mod grammar;
pub mod grammar_cli;
pub mod registry;
pub mod stable_ids;
mod tv;

pub use adapters::{Adapter, Session};
pub use grammar_cli::run_cmd as run_grammar_cmd;
pub use registry::{
    scan_sessions, watch_sessions, watch_sessions_cli, Harness, HealthSnapshot, SnapshotReason,
};

use control::adopt::adopt_session;
use events::{
    approve_session, deny_session, get_yolo, kill_session, list_sessions, send_prompt, set_yolo,
    spawn_session, start_watcher, AppState,
};
use stable_ids::StableIds;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, WindowEvent};

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
                total: std::sync::Mutex::new(0),
                sessions: std::sync::Mutex::new(Vec::new()),
                owned: std::sync::Mutex::new(owned_map),
                owned_path: std::sync::Mutex::new(owned_path),
                pending: std::sync::Mutex::new(HashMap::new()),
                approvals: std::sync::Mutex::new(HashMap::new()),
                yolo: std::sync::Mutex::new(false),
                yolo_seen: std::sync::Mutex::new(std::collections::HashSet::new()),
                ids: std::sync::Mutex::new(StableIds::new()),
            });
            app.manage(Arc::clone(&state));
            start_watcher(app.handle().clone(), state);
            Ok(())
        })
        .on_window_event(|window, event| {
            // M7g: close hides; backend (watcher/tick/yolo) keeps running.
            // ⌘Q / dock Quit still fire RunEvent::Exit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
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

    app.run(|app_handle, event| {
        match event {
            tauri::RunEvent::Exit => {
                // Log any owned sessions still working — tmux sessions survive deliberately.
                if let Some(state) = app_handle.try_state::<Arc<AppState>>() {
                    let snap = state.snapshot.lock().unwrap_or_else(|p| p.into_inner());
                    let owned = state.owned.lock().unwrap_or_else(|p| p.into_inner());
                    let working: Vec<_> = snap
                        .iter()
                        .filter(|s| owned.contains_key(&s.sid) && s.state == "working")
                        .map(|s| format!("{} ({})", s.n, s.title))
                        .collect();
                    if !working.is_empty() {
                        eprintln!(
                            "[exit] owned tmux sessions still working (left running): {}",
                            working.join(", ")
                        );
                    }
                }
                control::opencode::shutdown();
            }
            tauri::RunEvent::Reopen { .. } => {
                // Dock click while hidden — show the main window again.
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            _ => {}
        }
    });
}
