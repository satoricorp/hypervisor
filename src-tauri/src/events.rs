//! Tauri commands + session event stream for M2/M3.

use crate::adapters::Session;
use crate::approvals::{self, PendingApproval, ToastEvent};
use crate::control::archived::{self, ArchivedMap, ArchivedWire};
use crate::control::owned::OwnedMap;
use crate::control::titles::{self, TitlesMap};
use crate::control::{opencode, owned, tmux};
use crate::registry::{scan_sessions, watch_sessions, HealthSnapshot, SnapshotReason};
use crate::remote::SseBus;
use crate::stable_ids::{self, StableIds};
use crate::transcript::{self, TranscriptItem};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

/// Sidebar display cap — overflow footer reports anything beyond this.
const LIMIT: usize = 8;
/// Scan wider than LIMIT so `total` can be honest about overflow.
const SCAN_LIMIT: usize = 64;
const MAX_AGE_HOURS: f64 = 48.0;

/// Poisoning-tolerant lock — a panic in one critical section must not brick
/// the app. DECISION: small helper over parking_lot to avoid a new direct dep.
fn lock<'a, T>(m: &'a Mutex<T>) -> MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Serialize, Clone, Debug, PartialEq)]
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
    /// Stable session number (M7g) — process-lifetime, not sidebar position.
    pub n: u32,
    /// Pending approval letter A–Z when needs_you (M7g).
    pub letter: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SessionsUpdate {
    pub sessions: Vec<SessionWire>,
    pub total: usize,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct HealthEvent {
    pub watcher: bool,
    pub adapters: Vec<AdapterHealth>,
    pub serve: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct AdapterHealth {
    pub harness: String,
    pub status: String,
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
    pub total: Mutex<usize>,
    /// Last adapter sessions from the watcher (or a oneshot command scan).
    /// Tick re-finalizes these; emit paths must not full-rescan when this is set.
    pub sessions: Mutex<Vec<Session>>,
    pub owned: Mutex<OwnedMap>,
    pub owned_path: Mutex<PathBuf>,
    /// Local tombstones — sid → archived_at (unix secs). Never touches harness dirs.
    pub archived: Mutex<ArchivedMap>,
    pub archived_path: Mutex<PathBuf>,
    /// Local title overrides — sid → custom title. Never touches harness dirs.
    pub titles: Mutex<TitlesMap>,
    pub titles_path: Mutex<PathBuf>,
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
    /// Stable session numbers + approval letters (M7g).
    pub ids: Mutex<StableIds>,
    /// SSE broadcast for the M8a phone page.
    pub remote_bus: Arc<SseBus>,
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
    titles: &TitlesMap,
    ids: &mut StableIds,
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
            let n = ids.number_for(&s.sid);
            let letter = ids.letter_of_sid(&s.sid).map(|c| c.to_string());
            let title = titles
                .get(&s.sid)
                .cloned()
                .unwrap_or_else(|| s.title.clone());
            SessionWire {
                harness: s.harness.clone(),
                sid: s.sid.clone(),
                title,
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
                n,
                letter,
            }
        })
        .collect()
}

fn merge_pending(
    mut wire: Vec<SessionWire>,
    owned: &OwnedMap,
    pending: &HashMap<String, PendingOwned>,
    approvals: &HashMap<String, PendingApproval>,
    archived: &ArchivedMap,
    titles: &TitlesMap,
    ids: &mut StableIds,
) -> Vec<SessionWire> {
    let seen: std::collections::HashSet<_> = wire.iter().map(|s| s.sid.clone()).collect();
    for (sid, p) in pending {
        if seen.contains(sid) {
            continue;
        }
        if !owned.contains_key(sid) {
            continue;
        }
        // Same tombstone filter as adapter rows — never hide a living future.
        if archived::is_hidden(archived, sid, p.spawn_time) {
            continue;
        }
        let approval = approvals.get(sid).map(|a| a.text.clone());
        let state = if approval.is_some() {
            "needs_you".into()
        } else {
            "done".into()
        };
        let n = ids.number_for(sid);
        let letter = ids.letter_of_sid(sid).map(|c| c.to_string());
        let derived = format!("new session — {}", &sid[..8.min(sid.len())]);
        let title = titles.get(sid).cloned().unwrap_or(derived);
        wire.insert(
            0,
            SessionWire {
                harness: p.harness.clone(),
                sid: sid.clone(),
                title,
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
                n,
                letter,
            },
        );
    }
    wire
}

/// Drop tombstones whose mtime advanced past archived_at; filter the rest.
/// Persists when any tombstone is cleared (resurface).
fn filter_archived(state: &AppState, sessions: &mut Vec<Session>) {
    let mut archived = lock(&state.archived);
    let mut dirty = false;
    sessions.retain(|s| match archived.get(&s.sid).copied() {
        Some(at) if s.mtime > at => {
            archived.remove(&s.sid);
            dirty = true;
            true
        }
        Some(_) => false,
        None => true,
    });
    if dirty {
        let path = lock(&state.archived_path).clone();
        if let Err(e) = archived::save(&path, &archived) {
            eprintln!("archived.json save after resurface failed: {e}");
        }
    }
}

/// Build wire from cached sessions (or a provided list). Never full-rescans
/// when the watcher cache is warm. Returns (wire, total_before_display_cap).
fn apply_approvals_to_snapshot(
    state: &AppState,
    sessions: Option<Vec<Session>>,
) -> (Vec<SessionWire>, usize) {
    let owned = lock(&state.owned).clone();
    let pending = lock(&state.pending).clone();
    let approvals = lock(&state.approvals).clone();
    // Sync approval letters before building wire.
    {
        let mut live = HashMap::new();
        for (sid, p) in &approvals {
            live.insert(
                sid.clone(),
                stable_ids::approval_identity(
                    sid,
                    &p.source,
                    p.fingerprint.as_deref(),
                    &p.text,
                ),
            );
        }
        lock(&state.ids).sync_approvals(&live);
    }
    let mut sessions = match sessions {
        Some(s) => s,
        None => {
            let cached = lock(&state.sessions).clone();
            if !cached.is_empty() {
                cached
            } else {
                // Cold start before watcher has emitted — oneshot only.
                scan_sessions(MAX_AGE_HOURS, SCAN_LIMIT, None)
            }
        }
    };
    // Archive filter lives here (not in scan_sessions) so hvscan stays raw.
    filter_archived(state, &mut sessions);
    let archived = lock(&state.archived).clone();
    let titles = lock(&state.titles).clone();
    // Drop pending entries once adapters have the real row.
    {
        let mut p = lock(&state.pending);
        p.retain(|sid, _| !sessions.iter().any(|s| &s.sid == sid));
    }
    let total = sessions.len();
    let capped: Vec<Session> = sessions.into_iter().take(LIMIT).collect();
    let mut ids = lock(&state.ids);
    let wire = merge_pending(
        to_wire(&capped, &owned, &approvals, &titles, &mut ids),
        &owned,
        &pending,
        &approvals,
        &archived,
        &titles,
        &mut ids,
    );
    (wire, total)
}

fn emit_update(app: &AppHandle, state: &AppState, wire: Vec<SessionWire>, total: usize) {
    *lock(&state.snapshot) = wire.clone();
    *lock(&state.total) = total;
    let update = SessionsUpdate {
        sessions: wire,
        total,
    };
    let _ = app.emit("sessions:update", &update);
    crate::remote::broadcast_sessions(&state.remote_bus, &update);
}

pub(crate) fn emit_snapshot(app: &AppHandle, state: &AppState, sessions: Vec<Session>) {
    *lock(&state.sessions) = sessions.clone();
    let (wire, total) = apply_approvals_to_snapshot(state, Some(sessions));
    emit_update(app, state, wire, total);
}

fn emit_current(app: &AppHandle, state: &AppState) {
    let (wire, total) = apply_approvals_to_snapshot(state, None);
    emit_update(app, state, wire, total);
}

fn emit_if_changed(app: &AppHandle, state: &AppState, wire: Vec<SessionWire>, total: usize) {
    {
        let snap = lock(&state.snapshot);
        let prev_total = *lock(&state.total);
        if *snap == wire && prev_total == total {
            return;
        }
    }
    emit_update(app, state, wire, total);
}

fn emit_health(app: &AppHandle, health: &HealthSnapshot) {
    let degraded: std::collections::HashSet<&str> =
        health.degraded.iter().map(|s| s.as_str()).collect();
    let adapters = ["claude code", "codex", "cursor", "opencode"]
        .iter()
        .map(|h| AdapterHealth {
            harness: (*h).into(),
            status: if degraded.contains(h) {
                "degraded".into()
            } else {
                "ok".into()
            },
        })
        .collect();
    let _ = app.emit(
        "health",
        &HealthEvent {
            watcher: true,
            adapters,
            serve: opencode::healthy(),
        },
    );
}

/// Approval detection using cached session cwds — no adapter scans.
fn refresh_approvals(state: &AppState) -> Vec<(String, String)> {
    let owned = lock(&state.owned).clone();
    let sessions = lock(&state.sessions).clone();
    let prev = lock(&state.approvals).clone();

    let mut harness_by_sid = HashMap::new();
    let mut cwd_by_sid = HashMap::new();
    for s in &sessions {
        harness_by_sid.insert(s.sid.clone(), s.harness.clone());
        if s.harness == "opencode" && !s.cwd.is_empty() {
            cwd_by_sid.insert(s.sid.clone(), s.cwd.clone());
        }
    }
    // Also seed from pending owned placeholders + wire snapshot (sidebar).
    {
        let pending = lock(&state.pending);
        for (sid, p) in pending.iter() {
            harness_by_sid
                .entry(sid.clone())
                .or_insert_with(|| p.harness.clone());
            if p.harness == "opencode" && !p.cwd.is_empty() {
                cwd_by_sid.entry(sid.clone()).or_insert_with(|| p.cwd.clone());
            }
        }
    }
    {
        let snap = lock(&state.snapshot);
        for s in snap.iter() {
            harness_by_sid
                .entry(s.sid.clone())
                .or_insert_with(|| s.harness.clone());
            if s.harness == "opencode" && !s.cwd.is_empty() {
                cwd_by_sid.entry(s.sid.clone()).or_insert_with(|| s.cwd.clone());
            }
        }
    }

    let mut next: HashMap<String, PendingApproval> = HashMap::new();
    approvals::detect_opencode(&cwd_by_sid, &mut next);
    approvals::detect_tmux(&owned, &harness_by_sid, &prev, &mut next);

    let yolo = *lock(&state.yolo);
    let mut auto_toasts: Vec<(String, String)> = Vec::new();

    if yolo {
        let mut still = HashMap::new();
        let mut seen = lock(&state.yolo_seen);
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
        *lock(&state.approvals) = still;
    } else {
        lock(&state.yolo_seen).clear();
        // Log newly detected approvals (M7g window-closed proof).
        for (sid, p) in &next {
            if !prev.contains_key(sid) {
                eprintln!("[approval] detected sid={sid} wants={}", p.text);
            }
        }
        *lock(&state.approvals) = next;
    }

    auto_toasts
}

pub fn start_watcher(app: AppHandle, state: Arc<AppState>) {
    // Single emitter thread: fs events + 2s tick (approvals + re-finalize).
    thread::spawn(move || {
        let handle = app;
        let st = state;
        if let Err(e) = watch_sessions(MAX_AGE_HOURS, SCAN_LIMIT, move |sessions, reason, health| {
            *lock(&st.sessions) = sessions.clone();

            let auto_toasts = if reason == SnapshotReason::Tick {
                refresh_approvals(&st)
            } else {
                Vec::new()
            };

            let (wire, total) = apply_approvals_to_snapshot(&st, Some(sessions));
            match reason {
                SnapshotReason::Tick => emit_if_changed(&handle, &st, wire, total),
                SnapshotReason::Startup | SnapshotReason::Fs => {
                    emit_update(&handle, &st, wire, total);
                }
            }

            // Health on every tick (and startup) so serve/degraded flips show up.
            if reason == SnapshotReason::Tick || reason == SnapshotReason::Startup {
                emit_health(&handle, &health);
            }

            for (sid, text) in auto_toasts {
                let short = if sid.len() > 8 { &sid[..8] } else { &sid };
                let _ = handle.emit(
                    "toast",
                    &ToastEvent {
                        label: format!("yolo approved {short}"),
                        detail: Some(text),
                    },
                );
            }
        }) {
            eprintln!("session watcher failed: {e}");
        }
    });
}

#[tauri::command]
pub fn list_sessions(state: State<'_, Arc<AppState>>) -> SessionsUpdate {
    let snap = lock(&state.snapshot);
    let total = *lock(&state.total);
    if !snap.is_empty() {
        return SessionsUpdate {
            sessions: snap.clone(),
            total: if total == 0 { snap.len() } else { total },
        };
    }
    drop(snap);
    let (wire, total) = apply_approvals_to_snapshot(&state, None);
    *lock(&state.snapshot) = wire.clone();
    *lock(&state.total) = total;
    SessionsUpdate {
        sessions: wire,
        total,
    }
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
            let mut map = lock(&state.owned);
            map.insert(
                sid.clone(),
                owned::OwnedEntry::new(tmux_name.clone(), harness_label.clone()),
            );
            let path = lock(&state.owned_path).clone();
            owned::save(&path, &map)?;
        }
        lock(&state.pending).insert(
            sid.clone(),
            PendingOwned {
                harness: harness_label.clone(),
                model: model.clone(),
                cwd: cwd.clone(),
                tmux_name: tmux_name.clone(),
                spawn_time,
            },
        );
        let sessions = scan_sessions(MAX_AGE_HOURS, SCAN_LIMIT, None);
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
        // ~2s spawn health: dead pane → toast + scrub ghost placeholder.
        thread::sleep(Duration::from_secs(2));
        if tmux::pane_dead(&tmux_name2) {
            let detail = spawn_failure_detail(&tmux_name2);
            scrub_dead_spawn(&st, known_sid.as_deref(), &tmux_name2);
            let sessions = scan_sessions(MAX_AGE_HOURS, SCAN_LIMIT, None);
            emit_snapshot(&app2, &st, sessions);
            let _ = app2.emit(
                "toast",
                &ToastEvent {
                    label: format!("spawn failed · {tmux_name2}"),
                    detail: Some(detail),
                },
            );
            let _ = tmux::kill(&tmux_name2);
            return;
        }

        if known_sid.is_some() {
            for _ in 0..120 {
                thread::sleep(Duration::from_millis(500));
                if tmux::pane_dead(&tmux_name2) {
                    let detail = spawn_failure_detail(&tmux_name2);
                    scrub_dead_spawn(&st, known_sid.as_deref(), &tmux_name2);
                    let sessions = scan_sessions(MAX_AGE_HOURS, SCAN_LIMIT, None);
                    emit_snapshot(&app2, &st, sessions);
                    let _ = app2.emit(
                        "toast",
                        &ToastEvent {
                            label: format!("spawn failed · {tmux_name2}"),
                            detail: Some(detail),
                        },
                    );
                    let _ = tmux::kill(&tmux_name2);
                    return;
                }
                let sessions = scan_sessions(MAX_AGE_HOURS, SCAN_LIMIT, None);
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
                    let mut map = lock(&st.owned);
                    map.insert(
                        sid.clone(),
                        owned::OwnedEntry::new(tmux_name2.clone(), harness_label2.clone()),
                    );
                    let path = lock(&st.owned_path).clone();
                    if let Err(e) = owned::save(&path, &map) {
                        eprintln!("owned.json save failed: {e}");
                    }
                }
                let sessions = scan_sessions(MAX_AGE_HOURS, SCAN_LIMIT, None);
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

fn spawn_failure_detail(tmux_name: &str) -> String {
    match tmux::capture_pane(tmux_name, -40) {
        Ok(pane) => {
            let lines: Vec<&str> = pane
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect();
            let take = lines.len().saturating_sub(6);
            let tail = lines[take..].join(" · ");
            if tail.is_empty() {
                "pane exited with no output".into()
            } else {
                tail
            }
        }
        Err(_) => "pane exited (no capture)".into(),
    }
}

fn scrub_dead_spawn(state: &AppState, sid: Option<&str>, tmux_name: &str) {
    if let Some(sid) = sid {
        lock(&state.pending).remove(sid);
        let mut map = lock(&state.owned);
        map.remove(sid);
        let path = lock(&state.owned_path).clone();
        if let Err(e) = owned::save(&path, &map) {
            eprintln!("owned.json save after dead spawn failed: {e}");
        }
    } else {
        // Sid unknown — drop any owned entry pointing at this tmux name.
        let mut map = lock(&state.owned);
        let victims: Vec<String> = map
            .iter()
            .filter(|(_, e)| e.tmux == tmux_name)
            .map(|(k, _)| k.clone())
            .collect();
        for sid in &victims {
            map.remove(sid);
            lock(&state.pending).remove(sid);
        }
        if !victims.is_empty() {
            let path = lock(&state.owned_path).clone();
            if let Err(e) = owned::save(&path, &map) {
                eprintln!("owned.json save after dead spawn failed: {e}");
            }
        }
    }
}

#[tauri::command]
pub fn send_prompt(
    state: State<'_, Arc<AppState>>,
    sid: String,
    text: String,
) -> Result<(), String> {
    {
        let map = lock(&state.owned);
        if let Some(entry) = map.get(&sid).cloned() {
            drop(map);
            return tmux::send(&entry.tmux, &text);
        }
    }

    let sess = {
        let snap = lock(&state.snapshot);
        snap.iter().find(|s| s.sid == sid).cloned()
    };
    let sess = match sess {
        Some(s) => s,
        None => {
            let owned = lock(&state.owned).clone();
            let approvals = lock(&state.approvals).clone();
            let sessions = {
                let cached = lock(&state.sessions).clone();
                if !cached.is_empty() {
                    cached
                } else {
                    scan_sessions(MAX_AGE_HOURS, SCAN_LIMIT, None)
                }
            };
            to_wire(
                &sessions,
                &owned,
                &approvals,
                &lock(&state.titles),
                &mut lock(&state.ids),
            )
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
    let map = lock(&state.owned);
    let target = map
        .get(&sid)
        .map(|e| e.tmux.clone())
        .ok_or_else(|| "session is not owned by hypervisor tmux".to_string())?;
    drop(map);
    tmux::kill(&target)
}

/// Effective wire state for a sid (approvals → needs_you), from snapshot or cache.
fn effective_state(state: &AppState, sid: &str) -> Option<String> {
    {
        let snap = lock(&state.snapshot);
        if let Some(s) = snap.iter().find(|s| s.sid == sid) {
            return Some(s.state.clone());
        }
    }
    let sessions = lock(&state.sessions);
    let s = sessions.iter().find(|s| s.sid == sid)?;
    if lock(&state.approvals).contains_key(sid) {
        Some("needs_you".into())
    } else {
        Some(s.state.clone())
    }
}

/// Tombstone + optional tmux teardown for an owned idle session.
fn archive_one(state: &AppState, sid: &str) -> Result<String, String> {
    let st = effective_state(state, sid).unwrap_or_else(|| "done".into());
    if st == "working" {
        return Err("session is working — wait for it to finish".into());
    }
    // needs_you is skippable by archive_idle but a direct archive is allowed
    // (user explicitly chose this row). Only working is refused.

    let mut toast = "archived".to_string();
    let tmux_name = {
        let map = lock(&state.owned);
        map.get(sid).map(|e| e.tmux.clone())
    };
    if let Some(name) = tmux_name {
        let _ = tmux::kill(&name);
        {
            let mut map = lock(&state.owned);
            map.remove(sid);
            let path = lock(&state.owned_path).clone();
            if let Err(e) = owned::save(&path, &map) {
                eprintln!("owned.json save after archive failed: {e}");
            }
        }
        toast =
            "archived — tmux session closed; context stays in the transcript".into();
    }

    {
        let mut archived = lock(&state.archived);
        archived.insert(sid.to_string(), archived::now_secs());
        let path = lock(&state.archived_path).clone();
        archived::save(&path, &archived)?;
    }
    Ok(toast)
}

#[tauri::command]
pub fn archive_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    sid: String,
) -> Result<String, String> {
    let msg = archive_one(&state, &sid)?;
    emit_current(&app, &state);
    Ok(msg)
}

#[tauri::command]
pub fn unarchive_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    sid: String,
) -> Result<(), String> {
    {
        let mut archived = lock(&state.archived);
        archived.remove(&sid);
        let path = lock(&state.archived_path).clone();
        archived::save(&path, &archived)?;
    }
    emit_current(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn list_archived(state: State<'_, Arc<AppState>>) -> Vec<ArchivedWire> {
    let archived = lock(&state.archived).clone();
    if archived.is_empty() {
        return Vec::new();
    }
    // Unfiltered scan so titles/harnesses resolve even for hidden rows.
    let sessions = {
        let cached = lock(&state.sessions).clone();
        if !cached.is_empty() {
            cached
        } else {
            scan_sessions(MAX_AGE_HOURS, SCAN_LIMIT, None)
        }
    };
    let by_sid: HashMap<&str, &Session> =
        sessions.iter().map(|s| (s.sid.as_str(), s)).collect();
    let titles = lock(&state.titles).clone();
    let mut out: Vec<ArchivedWire> = archived
        .iter()
        .map(|(sid, &at)| {
            let (derived, harness) = match by_sid.get(sid.as_str()) {
                Some(s) => (s.title.clone(), s.harness.clone()),
                None => (sid.clone(), String::new()),
            };
            let title = titles.get(sid).cloned().unwrap_or(derived);
            ArchivedWire {
                sid: sid.clone(),
                title,
                harness,
                archived_at: at,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.archived_at
            .partial_cmp(&a.archived_at)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

#[tauri::command]
pub fn archive_idle(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<u32, String> {
    // Build effective states from the unfiltered cache + approvals.
    let sessions = lock(&state.sessions).clone();
    let approvals = lock(&state.approvals).clone();
    let archived = lock(&state.archived).clone();
    let mut count = 0u32;
    for s in &sessions {
        if archived::is_hidden(&archived, &s.sid, s.mtime) {
            continue;
        }
        let st = if approvals.contains_key(&s.sid) {
            "needs_you"
        } else {
            s.state.as_str()
        };
        if st == "working" || st == "needs_you" {
            continue;
        }
        if st == "done" || st == "stalled" {
            match archive_one(&state, &s.sid) {
                Ok(_) => count += 1,
                Err(_) => {} // working race — skip
            }
        }
    }
    emit_current(&app, &state);
    Ok(count)
}

/// On-demand transcript for the selected session. Does not touch the hot loop.
#[tauri::command]
pub fn get_transcript(
    state: State<'_, Arc<AppState>>,
    sid: String,
    limit: Option<u32>,
) -> Result<Vec<TranscriptItem>, String> {
    let limit = limit.unwrap_or(400) as usize;
    let (harness, src) = resolve_src(&state, &sid)?;
    Ok(transcript::parse_transcript(&src, &harness, limit))
}

fn resolve_src(state: &AppState, sid: &str) -> Result<(String, String), String> {
    {
        let sessions = lock(&state.sessions);
        if let Some(s) = sessions.iter().find(|s| s.sid == sid) {
            if !s.src.is_empty() {
                return Ok((s.harness.clone(), s.src.clone()));
            }
        }
    }
    {
        let snap = lock(&state.snapshot);
        if let Some(s) = snap.iter().find(|s| s.sid == sid) {
            if !s.src.is_empty() {
                return Ok((s.harness.clone(), s.src.clone()));
            }
        }
    }
    // Fallback: look for claude jsonl by sid (observe rows may be capped out).
    let home = crate::adapters::home_dir();
    let pattern = format!("{home}/.claude/projects/*/{sid}.jsonl");
    if let Ok(paths) = glob::glob(&pattern) {
        for p in paths.flatten() {
            return Ok((
                "claude code".into(),
                p.to_string_lossy().to_string(),
            ));
        }
    }
    Err(format!("no transcript source for session {sid}"))
}

/// Local title override. Empty or "-" clears. Never writes harness dirs.
#[tauri::command]
pub fn rename_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    sid: String,
    title: String,
) -> Result<String, String> {
    let trimmed = title.trim().to_string();
    let clear = trimmed.is_empty() || trimmed == "-";
    {
        let mut titles = lock(&state.titles);
        if clear {
            titles.remove(&sid);
        } else {
            titles.insert(sid.clone(), trimmed.clone());
        }
        let path = lock(&state.titles_path).clone();
        titles::save(&path, &titles)?;
    }
    emit_current(&app, &state);
    if clear {
        Ok("title reverted to derived".into())
    } else {
        Ok(format!("renamed — {trimmed}"))
    }
}

#[tauri::command]
pub fn approve_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    sid: String,
) -> Result<(), String> {
    approve_sid(&app, &state, &sid)
}

#[tauri::command]
pub fn deny_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    sid: String,
    guidance: String,
) -> Result<(), String> {
    deny_sid(&app, &state, &sid, &guidance)
}

#[tauri::command]
pub fn set_yolo(state: State<'_, Arc<AppState>>, on: bool) -> Result<(), String> {
    *lock(&state.yolo) = on;
    if !on {
        lock(&state.yolo_seen).clear();
    }
    Ok(())
}

#[tauri::command]
pub fn get_yolo(state: State<'_, Arc<AppState>>) -> bool {
    *lock(&state.yolo)
}

// ——— helpers for the M8a remote HTTP layer (same code path as tauri cmds) ———

pub fn current_sessions(state: &AppState) -> SessionsUpdate {
    let snap = lock(&state.snapshot);
    let total = *lock(&state.total);
    if !snap.is_empty() {
        return SessionsUpdate {
            sessions: snap.clone(),
            total: if total == 0 { snap.len() } else { total },
        };
    }
    drop(snap);
    let (wire, total) = apply_approvals_to_snapshot(state, None);
    *lock(&state.snapshot) = wire.clone();
    *lock(&state.total) = total;
    SessionsUpdate {
        sessions: wire,
        total,
    }
}

pub fn ids_snapshot(state: &AppState) -> StableIds {
    lock(&state.ids).clone()
}

pub fn approve_sid(app: &AppHandle, state: &AppState, sid: &str) -> Result<(), String> {
    let pending = {
        let map = lock(&state.approvals);
        map.get(sid)
            .cloned()
            .ok_or_else(|| "nothing pending approval on this session".to_string())?
    };
    let tmux_target = lock(&state.owned).get(sid).map(|e| e.tmux.clone());
    approvals::approve(&pending, tmux_target.as_deref())?;
    lock(&state.approvals).remove(sid);
    emit_current(app, state);
    Ok(())
}

pub fn deny_sid(
    app: &AppHandle,
    state: &AppState,
    sid: &str,
    guidance: &str,
) -> Result<(), String> {
    let pending = {
        let map = lock(&state.approvals);
        map.get(sid)
            .cloned()
            .ok_or_else(|| "nothing pending approval on this session".to_string())?
    };
    let tmux_target = lock(&state.owned).get(sid).map(|e| e.tmux.clone());
    approvals::deny(&pending, guidance, tmux_target.as_deref())?;
    lock(&state.approvals).remove(sid);
    emit_current(app, state);
    Ok(())
}

pub fn prompt_sid(state: &AppState, sid: &str, text: &str) -> Result<(), String> {
    {
        let map = lock(&state.owned);
        if let Some(entry) = map.get(sid).cloned() {
            drop(map);
            return tmux::send(&entry.tmux, text);
        }
    }
    let sess = {
        let snap = lock(&state.snapshot);
        snap.iter().find(|s| s.sid == sid).cloned()
    };
    let sess = match sess {
        Some(s) => s,
        None => {
            return Err(format!("session {sid} not found"));
        }
    };
    if sess.harness == "opencode" {
        return crate::control::opencode::prompt_async(sid, &sess.cwd, text);
    }
    Err("session is observe-only — adopt it on the desktop first".into())
}

pub fn any_owned_working(state: &AppState) -> bool {
    let owned = lock(&state.owned);
    let snap = lock(&state.snapshot);
    snap.iter()
        .any(|s| owned.contains_key(&s.sid) && s.state == "working")
}

#[cfg(test)]
mod archive_tests {
    use super::*;
    use crate::control::archived;
    use crate::remote::SseBus;
    use crate::stable_ids::StableIds;
    use std::fs;
    use std::sync::Arc;

    fn empty_state(dir: &Path) -> AppState {
        AppState {
            snapshot: Mutex::new(Vec::new()),
            total: Mutex::new(0),
            sessions: Mutex::new(Vec::new()),
            owned: Mutex::new(OwnedMap::new()),
            owned_path: Mutex::new(dir.join("owned.json")),
            archived: Mutex::new(ArchivedMap::new()),
            archived_path: Mutex::new(dir.join("archived.json")),
            titles: Mutex::new(TitlesMap::new()),
            titles_path: Mutex::new(dir.join("titles.json")),
            pending: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            yolo: Mutex::new(false),
            yolo_seen: Mutex::new(std::collections::HashSet::new()),
            ids: Mutex::new(StableIds::new()),
            remote_bus: Arc::new(SseBus::new()),
        }
    }

    fn sess(sid: &str, state: &str, mtime: f64) -> Session {
        Session {
            harness: "claude code".into(),
            sid: sid.into(),
            title: format!("t-{sid}"),
            model: "m".into(),
            cwd: "/tmp".into(),
            branch: "main".into(),
            last_user: String::new(),
            last_assistant: String::new(),
            activity: String::new(),
            mtime,
            state: state.into(),
            age: "1m".into(),
            repo: "r".into(),
            src: String::new(),
            sidechains: 0,
            last_role: "assistant".into(),
        }
    }

    #[test]
    fn filter_hides_and_resurfaces() {
        let dir = std::env::temp_dir().join(format!("hv-arch-ev-{}", archived::now_secs() as u64));
        fs::create_dir_all(&dir).unwrap();
        let state = empty_state(&dir);
        {
            let mut a = lock(&state.archived);
            a.insert("dead".into(), 100.0);
            a.insert("live".into(), 100.0);
            archived::save(&lock(&state.archived_path), &a).unwrap();
        }
        let mut sessions = vec![sess("dead", "done", 50.0), sess("live", "done", 150.0)];
        filter_archived(&state, &mut sessions);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].sid, "live");
        assert!(!lock(&state.archived).contains_key("live"), "resurface drops tombstone");
        assert!(lock(&state.archived).contains_key("dead"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_refuses_working() {
        let dir = std::env::temp_dir().join(format!("hv-arch-w-{}", archived::now_secs() as u64));
        fs::create_dir_all(&dir).unwrap();
        let state = empty_state(&dir);
        *lock(&state.sessions) = vec![sess("w1", "working", 50.0)];
        *lock(&state.snapshot) = to_wire(
            &lock(&state.sessions),
            &OwnedMap::new(),
            &HashMap::new(),
            &TitlesMap::new(),
            &mut lock(&state.ids),
        );
        let err = archive_one(&state, "w1").unwrap_err();
        assert!(err.contains("working"), "{err}");
        assert!(lock(&state.archived).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_idle_skips_working_and_needs_you() {
        let dir = std::env::temp_dir().join(format!("hv-arch-i-{}", archived::now_secs() as u64));
        fs::create_dir_all(&dir).unwrap();
        let state = empty_state(&dir);
        *lock(&state.sessions) = vec![
            sess("d1", "done", 10.0),
            sess("s1", "stalled", 10.0),
            sess("w1", "working", 10.0),
            sess("n1", "done", 10.0),
        ];
        lock(&state.approvals).insert(
            "n1".into(),
            PendingApproval {
                text: "run ls".into(),
                source: crate::approvals::ApprovalSource::Tmux,
                fingerprint: None,
            },
        );
        // archive_idle needs AppHandle — exercise selection logic inline
        let approvals = lock(&state.approvals).clone();
        let sids: Vec<(String, String)> = lock(&state.sessions)
            .iter()
            .map(|s| (s.sid.clone(), s.state.clone()))
            .collect();
        let mut n = 0u32;
        for (sid, state_name) in sids {
            let st = if approvals.contains_key(&sid) {
                "needs_you"
            } else {
                state_name.as_str()
            };
            if st == "working" || st == "needs_you" {
                continue;
            }
            if st == "done" || st == "stalled" {
                archive_one(&state, &sid).unwrap();
                n += 1;
            }
        }
        assert_eq!(n, 2);
        assert!(lock(&state.archived).contains_key("d1"));
        assert!(lock(&state.archived).contains_key("s1"));
        assert!(!lock(&state.archived).contains_key("w1"));
        assert!(!lock(&state.archived).contains_key("n1"));
        let _ = fs::remove_dir_all(&dir);
    }
}
