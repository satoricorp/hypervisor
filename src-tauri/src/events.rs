//! Tauri commands + session event stream for M2/M3.

use crate::adapters::Session;
use crate::approvals::{self, PendingApproval, ToastEvent};
use crate::control::{owned, tmux};
use crate::control::owned::OwnedMap;
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
const APPROVAL_POLL_MS: u64 = 2000;

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
    pub approval: Option<String>,
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
    pub owned: Mutex<OwnedMap>,
    pub owned_path: Mutex<PathBuf>,
    /// Placeholders for owned sids whose transcript isn't visible yet.
    pub pending: Mutex<HashMap<String, PendingOwned>>,
    /// sid → pending permission (M3).
    pub approvals: Mutex<HashMap<String, PendingApproval>>,
    // DECISION: yolo stays in-memory only — a forgotten toggle must not
    // auto-approve anything after a restart.
    pub yolo: Mutex<bool>,
    /// Keys already auto-approved this yolo session (request id / fingerprint)
    /// so we don't re-toast or hammer.
    pub yolo_seen: Mutex<std::collections::HashSet<String>>,
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

fn control_for(sid: &str, harness: &str, owned: &OwnedMap) -> String {
    if owned.contains_key(sid) {
        "tmux".into()
    } else if harness == "cursor" {
        "watch".into()
    } else if harness == "opencode" {
        "api".into()
    } else {
        "observe".into()
    }
}

fn to_wire(
    sessions: &[Session],
    owned: &OwnedMap,
    approvals: &HashMap<String, PendingApproval>,
) -> Vec<SessionWire> {
    sessions
        .iter()
        .map(|s| {
            let approval = approvals.get(&s.sid).map(|a| a.text.clone());
            let state = if approval.is_some() {
                "needs_you".into()
            } else {
                s.state.clone()
            };
            SessionWire {
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
                state,
                age: s.age.clone(),
                repo: s.repo.clone(),
                src: s.src.clone(),
                sidechains: s.sidechains,
                control: control_for(&s.sid, &s.harness, owned),
                approval,
            }
        })
        .collect()
}

fn merge_pending(
    mut wire: Vec<SessionWire>,
    owned: &OwnedMap,
    pending: &HashMap<String, PendingOwned>,
    approvals: &HashMap<String, PendingApproval>,
) -> Vec<SessionWire> {
    let seen: std::collections::HashSet<_> = wire.iter().map(|s| s.sid.clone()).collect();
    for (sid, p) in pending {
        if seen.contains(sid) {
            continue;
        }
        if !owned.contains_key(sid) {
            continue;
        }
        let approval = approvals.get(sid).map(|a| a.text.clone());
        let state = if approval.is_some() {
            "needs_you".into()
        } else {
            "done".into()
        };
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
                state,
                age: "now".into(),
                repo: repo_of(&p.cwd),
                src: String::new(),
                sidechains: 0,
                control: "tmux".into(),
                approval,
            },
        );
    }
    wire
}

fn apply_approvals_to_snapshot(
    state: &AppState,
    sessions: Option<Vec<Session>>,
) -> Vec<SessionWire> {
    let owned = state.owned.lock().unwrap().clone();
    let pending = state.pending.lock().unwrap().clone();
    let approvals = state.approvals.lock().unwrap().clone();
    let sessions = sessions.unwrap_or_else(|| scan_sessions(MAX_AGE_HOURS, LIMIT, None));
    // Drop pending entries once adapters have the real row.
    {
        let mut p = state.pending.lock().unwrap();
        p.retain(|sid, _| !sessions.iter().any(|s| &s.sid == sid));
    }
    merge_pending(to_wire(&sessions, &owned, &approvals), &owned, &pending, &approvals)
}

pub(crate) fn emit_snapshot(app: &AppHandle, state: &AppState, sessions: Vec<Session>) {
    let wire = apply_approvals_to_snapshot(state, Some(sessions));
    *state.snapshot.lock().unwrap() = wire.clone();
    let _ = app.emit("sessions:update", &wire);
}

fn emit_current(app: &AppHandle, state: &AppState) {
    let wire = apply_approvals_to_snapshot(state, None);
    *state.snapshot.lock().unwrap() = wire.clone();
    let _ = app.emit("sessions:update", &wire);
}

fn refresh_approvals(app: &AppHandle, state: &AppState) {
    let owned = state.owned.lock().unwrap().clone();
    let snap = state.snapshot.lock().unwrap().clone();
    let prev = state.approvals.lock().unwrap().clone();

    let mut harness_by_sid = HashMap::new();
    let mut cwd_by_sid = HashMap::new();
    for s in &snap {
        harness_by_sid.insert(s.sid.clone(), s.harness.clone());
        if s.harness == "opencode" && !s.cwd.is_empty() {
            cwd_by_sid.insert(s.sid.clone(), s.cwd.clone());
        }
    }
    // Also seed harness from pending owned placeholders.
    {
        let pending = state.pending.lock().unwrap();
        for (sid, p) in pending.iter() {
            harness_by_sid
                .entry(sid.clone())
                .or_insert_with(|| p.harness.clone());
            if p.harness == "opencode" && !p.cwd.is_empty() {
                cwd_by_sid.entry(sid.clone()).or_insert_with(|| p.cwd.clone());
            }
        }
    }

    // Seed opencode cwds from a fresh harness scan (sidebar limit may omit some).
    for s in scan_sessions(MAX_AGE_HOURS, 64, Some(crate::registry::Harness::Opencode)) {
        if !s.cwd.is_empty() {
            cwd_by_sid.entry(s.sid.clone()).or_insert(s.cwd);
        }
    }

    let mut next: HashMap<String, PendingApproval> = HashMap::new();
    approvals::detect_opencode(&cwd_by_sid, &mut next);
    approvals::detect_tmux(&owned, &harness_by_sid, &prev, &mut next);

    let yolo = *state.yolo.lock().unwrap();
    let mut auto_toasts: Vec<(String, String)> = Vec::new();

    if yolo {
        let mut still = HashMap::new();
        let mut seen = state.yolo_seen.lock().unwrap();
        for (sid, pending) in next {
            let key = match &pending.source {
                approvals::ApprovalSource::Opencode { request_id, .. } => {
                    format!("oc:{request_id}")
                }
                approvals::ApprovalSource::Tmux => {
                    format!(
                        "tmux:{sid}:{}",
                        pending.fingerprint.as_deref().unwrap_or(&pending.text)
                    )
                }
            };
            if seen.contains(&key) {
                // Already auto-approved — wait for detection to drop it.
                continue;
            }
            let tmux_target = owned.get(&sid).map(|e| e.tmux.as_str());
            match approvals::approve(&pending, tmux_target) {
                Ok(()) => {
                    seen.insert(key);
                    auto_toasts.push((sid.clone(), pending.text.clone()));
                }
                Err(e) => {
                    eprintln!("yolo approve failed for {sid}: {e}");
                    still.insert(sid, pending);
                }
            }
        }
        drop(seen);
        *state.approvals.lock().unwrap() = still;
    } else {
        state.yolo_seen.lock().unwrap().clear();
        *state.approvals.lock().unwrap() = next;
    }

    emit_current(app, state);

    for (sid, text) in auto_toasts {
        let short = if sid.len() > 8 { &sid[..8] } else { &sid };
        let html = format!(
            "yolo approved <b>{}</b> · {}",
            escape_html(short),
            escape_html(&text)
        );
        let _ = app.emit("toast", &ToastEvent { html });
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn start_watcher(app: AppHandle, state: Arc<AppState>) {
    // FS watcher thread (existing).
    {
        let handle = app.clone();
        let st = Arc::clone(&state);
        thread::spawn(move || {
            if let Err(e) = watch_sessions(MAX_AGE_HOURS, LIMIT, move |sessions| {
                emit_snapshot(&handle, &st, sessions);
            }) {
                eprintln!("session watcher failed: {e}");
            }
        });
    }
    // Approval poller — every 2s (opencode GET /permission + tmux panes).
    {
        let handle = app.clone();
        let st = Arc::clone(&state);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(APPROVAL_POLL_MS));
            refresh_approvals(&handle, &st);
        });
    }
}

#[tauri::command]
pub fn list_sessions(state: State<'_, Arc<AppState>>) -> Vec<SessionWire> {
    let snap = state.snapshot.lock().unwrap();
    if !snap.is_empty() {
        return snap.clone();
    }
    drop(snap);
    let wire = apply_approvals_to_snapshot(&state, None);
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
    let harness_label = if harness == "claude" {
        "claude code".to_string()
    } else {
        harness.clone()
    };

    if let Some(sid) = &spawned.sid {
        {
            let mut map = state.owned.lock().unwrap();
            map.insert(
                sid.clone(),
                owned::OwnedEntry::new(tmux_name.clone(), harness_label.clone()),
            );
            let path = state.owned_path.lock().unwrap().clone();
            owned::save(&path, &map)?;
        }
        state.pending.lock().unwrap().insert(
            sid.clone(),
            PendingOwned {
                harness: harness_label.clone(),
                model: model.clone(),
                cwd: cwd.clone(),
                tmux_name: tmux_name.clone(),
                spawn_time,
            },
        );
        let sessions = scan_sessions(MAX_AGE_HOURS, LIMIT, None);
        emit_snapshot(&app, &state, sessions);
    }

    let app2 = app.clone();
    let st = Arc::clone(state.inner());
    let harness2 = harness.clone();
    let harness_label2 = harness_label.clone();
    let cwd2 = cwd.clone();
    let tmux_name2 = tmux_name.clone();
    let known_sid = spawned.sid.clone();
    thread::spawn(move || {
        if known_sid.is_some() {
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
                    map.insert(
                        sid.clone(),
                        owned::OwnedEntry::new(tmux_name2.clone(), harness_label2.clone()),
                    );
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
    {
        let map = state.owned.lock().unwrap();
        if let Some(entry) = map.get(&sid).cloned() {
            drop(map);
            return tmux::send(&entry.tmux, &text);
        }
    }

    let sess = {
        let snap = state.snapshot.lock().unwrap();
        snap.iter().find(|s| s.sid == sid).cloned()
    };
    let sess = match sess {
        Some(s) => s,
        None => {
            let owned = state.owned.lock().unwrap().clone();
            let approvals = state.approvals.lock().unwrap().clone();
            let sessions = scan_sessions(MAX_AGE_HOURS, LIMIT, None);
            to_wire(&sessions, &owned, &approvals)
                .into_iter()
                .find(|s| s.sid == sid)
                .ok_or_else(|| format!("session {sid} not found"))?
        }
    };

    if sess.harness == "opencode" {
        // DECISION: no idle guard on the api path — HTTP is opencode's
        // concurrent-access surface; nothing forks. (Adopt fork guard for
        // claude/codex --resume is unchanged.)
        return crate::control::opencode::prompt_async(&sid, &sess.cwd, &text);
    }

    Err("session is observe-only — press ⏎ to adopt it first".into())
}

#[tauri::command]
pub fn kill_session(
    state: State<'_, Arc<AppState>>,
    sid: String,
) -> Result<(), String> {
    let map = state.owned.lock().unwrap();
    let target = map
        .get(&sid)
        .map(|e| e.tmux.clone())
        .ok_or_else(|| "session is not owned by hypervisor tmux".to_string())?;
    drop(map);
    tmux::kill(&target)
}

#[tauri::command]
pub fn approve_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    sid: String,
) -> Result<(), String> {
    let pending = {
        let map = state.approvals.lock().unwrap();
        map.get(&sid)
            .cloned()
            .ok_or_else(|| "nothing pending approval on this session".to_string())?
    };
    let tmux_target = state
        .owned
        .lock()
        .unwrap()
        .get(&sid)
        .map(|e| e.tmux.clone());
    approvals::approve(&pending, tmux_target.as_deref())?;
    state.approvals.lock().unwrap().remove(&sid);
    emit_current(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn deny_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    sid: String,
    guidance: String,
) -> Result<(), String> {
    let pending = {
        let map = state.approvals.lock().unwrap();
        map.get(&sid)
            .cloned()
            .ok_or_else(|| "nothing pending approval on this session".to_string())?
    };
    let tmux_target = state
        .owned
        .lock()
        .unwrap()
        .get(&sid)
        .map(|e| e.tmux.clone());
    approvals::deny(&pending, &guidance, tmux_target.as_deref())?;
    state.approvals.lock().unwrap().remove(&sid);
    emit_current(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_yolo(state: State<'_, Arc<AppState>>, on: bool) -> Result<(), String> {
    *state.yolo.lock().unwrap() = on;
    if !on {
        state.yolo_seen.lock().unwrap().clear();
    }
    Ok(())
}

#[tauri::command]
pub fn get_yolo(state: State<'_, Arc<AppState>>) -> bool {
    *state.yolo.lock().unwrap()
}
