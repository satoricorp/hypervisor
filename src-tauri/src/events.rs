//! Tauri commands + session event stream for M2.

use crate::adapters::Session;
use crate::control::{owned, tmux};
use crate::registry::{scan_sessions, watch_sessions};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

const MAX_AGE_HOURS: f64 = 48.0;
const LIMIT: usize = 8;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct SessionWire {
    pub harness: String,
    pub sid: String,
    pub title: String,
    pub model: String,
    pub cwd: String,
    pub branch: String,
    pub last_user: String,
    pub last_assistant: String,
    pub activity: String,
    pub mtime: f64,
    pub state: String,
    pub age: String,
    pub repo: String,
    pub src: String,
    pub sidechains: u32,
    pub control: String,
}

#[derive(Clone, Debug)]
pub struct PendingOwned {
    pub harness: String,
    pub model: String,
    pub cwd: String,
    pub tmux_name: String,
    pub spawn_time: f64,
}

pub struct AppState {
    pub snapshot: Mutex<Vec<SessionWire>>,
    pub owned: Mutex<HashMap<String, String>>,
    pub owned_path: Mutex<PathBuf>,
    /// Placeholders for owned sids whose transcript isn't visible yet.
    pub pending: Mutex<HashMap<String, PendingOwned>>,
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn repo_of(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("-")
        .to_string()
}

fn control_for(sid: &str, harness: &str, owned: &HashMap<String, String>) -> String {
    if owned.contains_key(sid) {
        "tmux".into()
    } else if harness == "cursor" {
        "watch".into()
    } else {
        "observe".into()
    }
}

fn to_wire(sessions: &[Session], owned: &HashMap<String, String>) -> Vec<SessionWire> {
    sessions
        .iter()
        .map(|s| SessionWire {
            harness: s.harness.clone(),
            sid: s.sid.clone(),
            title: s.title.clone(),
            model: s.model.clone(),
            cwd: s.cwd.clone(),
            branch: s.branch.clone(),
            last_user: s.last_user.clone(),
            last_assistant: s.last_assistant.clone(),
            activity: s.activity.clone(),
            mtime: s.mtime,
            state: s.state.clone(),
            age: s.age.clone(),
            repo: s.repo.clone(),
            src: s.src.clone(),
            sidechains: s.sidechains,
            control: control_for(&s.sid, &s.harness, owned),
        })
        .collect()
}

fn merge_pending(
    mut wire: Vec<SessionWire>,
    owned: &HashMap<String, String>,
    pending: &HashMap<String, PendingOwned>,
) -> Vec<SessionWire> {
    let seen: std::collections::HashSet<_> = wire.iter().map(|s| s.sid.clone()).collect();
    for (sid, p) in pending {
        if seen.contains(sid) {
            continue;
        }
        if !owned.contains_key(sid) {
            continue;
        }
        wire.insert(
            0,
            SessionWire {
                harness: p.harness.clone(),
                sid: sid.clone(),
                title: format!("new session — {}", &sid[..8.min(sid.len())]),
                model: p.model.clone(),
                cwd: p.cwd.clone(),
                branch: String::new(),
                last_user: String::new(),
                last_assistant: String::new(),
                activity: String::new(),
                mtime: p.spawn_time,
                state: "done".into(),
                age: "now".into(),
                repo: repo_of(&p.cwd),
                src: String::new(),
                sidechains: 0,
                control: "tmux".into(),
            },
        );
    }
    wire
}

pub(crate) fn emit_snapshot(app: &AppHandle, state: &AppState, sessions: Vec<Session>) {
    let owned = state.owned.lock().unwrap().clone();
    let mut pending = state.pending.lock().unwrap();
    // Drop pending entries once adapters have the real row.
    pending.retain(|sid, _| !sessions.iter().any(|s| &s.sid == sid));
    let wire = merge_pending(to_wire(&sessions, &owned), &owned, &pending);
    drop(pending);
    *state.snapshot.lock().unwrap() = wire.clone();
    let _ = app.emit("sessions:update", &wire);
}

pub fn start_watcher(app: AppHandle, state: Arc<AppState>) {
    thread::spawn(move || {
        let handle = app.clone();
        let st = Arc::clone(&state);
        if let Err(e) = watch_sessions(MAX_AGE_HOURS, LIMIT, move |sessions| {
            emit_snapshot(&handle, &st, sessions);
        }) {
            eprintln!("session watcher failed: {e}");
        }
    });
}

#[tauri::command]
pub fn list_sessions(state: State<'_, Arc<AppState>>) -> Vec<SessionWire> {
    let snap = state.snapshot.lock().unwrap();
    if !snap.is_empty() {
        return snap.clone();
    }
    drop(snap);
    let owned = state.owned.lock().unwrap().clone();
    let pending = state.pending.lock().unwrap().clone();
    let sessions = scan_sessions(MAX_AGE_HOURS, LIMIT, None);
    let wire = merge_pending(to_wire(&sessions, &owned), &owned, &pending);
    *state.snapshot.lock().unwrap() = wire.clone();
    wire
}

#[tauri::command]
pub fn spawn_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    harness: String,
    model: String,
    cwd: Option<String>,
) -> Result<String, String> {
    let cwd = cwd.unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/".into()));
    let spawn_time = now_secs();
    let spawned = tmux::spawn(&harness, &model, &cwd)?;
    let tmux_name = spawned.tmux_name.clone();

    if let Some(sid) = &spawned.sid {
        {
            let mut map = state.owned.lock().unwrap();
            map.insert(sid.clone(), tmux_name.clone());
            let path = state.owned_path.lock().unwrap().clone();
            owned::save(&path, &map)?;
        }
        state.pending.lock().unwrap().insert(
            sid.clone(),
            PendingOwned {
                harness: if harness == "claude" {
                    "claude code".into()
                } else {
                    harness.clone()
                },
                model: model.clone(),
                cwd: cwd.clone(),
                tmux_name: tmux_name.clone(),
                spawn_time,
            },
        );
        // Emit immediately so the sidebar shows the placeholder.
        let sessions = scan_sessions(MAX_AGE_HOURS, LIMIT, None);
        emit_snapshot(&app, &state, sessions);
    }

    let app2 = app.clone();
    let st = Arc::clone(state.inner());
    let harness2 = harness.clone();
    let cwd2 = cwd.clone();
    let tmux_name2 = tmux_name.clone();
    let known_sid = spawned.sid.clone();
    thread::spawn(move || {
        if known_sid.is_some() {
            // Poll until adapters see the transcript (after first prompt), or 60s.
            for _ in 0..120 {
                thread::sleep(Duration::from_millis(500));
                let sessions = scan_sessions(MAX_AGE_HOURS, LIMIT, None);
                let found = sessions.iter().any(|s| Some(&s.sid) == known_sid.as_ref());
                emit_snapshot(&app2, &st, sessions);
                if found {
                    return;
                }
            }
            return;
        }
        match owned::wait_for_sid(&harness2, &cwd2, spawn_time) {
            Some(sid) => {
                {
                    let mut map = st.owned.lock().unwrap();
                    map.insert(sid.clone(), tmux_name2.clone());
                    let path = st.owned_path.lock().unwrap().clone();
                    if let Err(e) = owned::save(&path, &map) {
                        eprintln!("owned.json save failed: {e}");
                    }
                }
                let sessions = scan_sessions(MAX_AGE_HOURS, LIMIT, None);
                emit_snapshot(&app2, &st, sessions);
            }
            None => {
                eprintln!(
                    "warning: could not correlate tmux session {tmux_name2} \
                     with a transcript within 15s — leaving as observe"
                );
            }
        }
    });

    Ok(tmux_name)
}

#[tauri::command]
pub fn send_prompt(
    state: State<'_, Arc<AppState>>,
    sid: String,
    text: String,
) -> Result<(), String> {
    let map = state.owned.lock().unwrap();
    let target = map.get(&sid).cloned().ok_or_else(|| {
        "session is observe-only — press ⏎ to adopt it first".to_string()
    })?;
    drop(map);
    tmux::send(&target, &text)
}

#[tauri::command]
pub fn kill_session(
    state: State<'_, Arc<AppState>>,
    sid: String,
) -> Result<(), String> {
    let map = state.owned.lock().unwrap();
    let target = map
        .get(&sid)
        .cloned()
        .ok_or_else(|| "session is not owned by hypervisor tmux".to_string())?;
    drop(map);
    tmux::kill(&target)
}
