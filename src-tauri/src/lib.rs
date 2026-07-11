mod access;
mod adapters;
pub mod approvals;
pub mod control;
pub mod events;
pub mod grammar;
pub mod grammar_cli;
pub mod registry;
pub mod remote;
pub mod stable_ids;
mod transcript;
mod tv;

pub use adapters::{Adapter, Session};
pub use grammar_cli::run_cmd as run_grammar_cmd;
pub use registry::{
    scan_sessions, watch_sessions, watch_sessions_cli, Harness, HealthSnapshot, SnapshotReason,
};

use control::adopt::adopt_session;
use events::{
    approve_session, archive_idle, archive_session, broadcast_prompt, compact_session,
    deny_session, get_access, get_settings, get_transcript, get_yolo, kill_session, list_archived,
    list_history, list_sessions, rename_session, send_prompt, set_settings, set_yolo, spawn_session,
    start_watcher, unarchive_session, AppState,
};
use remote::remote_status;
use stable_ids::StableIds;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let owned_path = data_dir.join("owned.json");
            let owned_map = control::owned::load(&owned_path);
            let archived_path = data_dir.join("archived.json");
            let archived_map = control::archived::load(&archived_path);
            let titles_path = data_dir.join("titles.json");
            let titles_map = control::titles::load(&titles_path);
            let settings_path = data_dir.join("settings.json");
            let settings = control::settings::load(&settings_path);
            let state = Arc::new(AppState {
                snapshot: std::sync::Mutex::new(Vec::new()),
                total: std::sync::Mutex::new(0),
                sessions: std::sync::Mutex::new(Vec::new()),
                owned: std::sync::Mutex::new(owned_map),
                owned_path: std::sync::Mutex::new(owned_path),
                archived: std::sync::Mutex::new(archived_map),
                archived_path: std::sync::Mutex::new(archived_path),
                titles: std::sync::Mutex::new(titles_map),
                titles_path: std::sync::Mutex::new(titles_path),
                settings: std::sync::Mutex::new(settings),
                settings_path: std::sync::Mutex::new(settings_path),
                pending: std::sync::Mutex::new(HashMap::new()),
                approvals: std::sync::Mutex::new(HashMap::new()),
                yolo: std::sync::Mutex::new(false),
                yolo_seen: std::sync::Mutex::new(std::collections::HashSet::new()),
                ids: std::sync::Mutex::new(StableIds::new()),
                remote_bus: Arc::new(remote::SseBus::new()),
            });
            app.manage(Arc::clone(&state));
            start_watcher(app.handle().clone(), Arc::clone(&state));
            remote::start(app.handle().clone(), Arc::clone(&state));
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
            compact_session,
            broadcast_prompt,
            adopt_session,
            approve_session,
            deny_session,
            set_yolo,
            get_yolo,
            archive_session,
            unarchive_session,
            list_archived,
            archive_idle,
            get_transcript,
            rename_session,
            get_settings,
            set_settings,
            get_access,
            list_history,
            remote_status,
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
