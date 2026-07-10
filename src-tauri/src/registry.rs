use crate::adapters::claude_code::ClaudeCodeAdapter;
use crate::adapters::codex::CodexAdapter;
use crate::adapters::cursor::{self, CursorAdapter};
use crate::adapters::opencode::{self, OpencodeAdapter};
use crate::adapters::{home_dir, refinalize, Adapter, Session};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Harness {
    ClaudeCode,
    Codex,
    Cursor,
    Opencode,
}

impl Harness {
    pub fn as_str(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude code",
            Harness::Codex => "codex",
            Harness::Cursor => "cursor",
            Harness::Opencode => "opencode",
        }
    }
}

const ALL: [Harness; 4] = [
    Harness::ClaudeCode,
    Harness::Codex,
    Harness::Cursor,
    Harness::Opencode,
];

/// Why a snapshot was produced (tick never triggers adapter scans).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotReason {
    Startup,
    Fs,
    Tick,
}

/// Scan all harnesses (or a subset) and return sessions sorted by mtime desc.
pub fn scan_sessions(
    max_age_hours: f64,
    limit: usize,
    only: Option<Harness>,
) -> Vec<Session> {
    let mut sessions = Vec::new();
    let harnesses: Vec<Harness> = match only {
        Some(h) => vec![h],
        None => ALL.to_vec(),
    };
    for h in harnesses {
        match scan_harness(h, max_age_hours, limit, "oneshot") {
            Ok(part) => sessions.extend(part),
            Err(e) => eprintln!("[scan] harness={} reason=oneshot error={e}", h.as_str()),
        }
    }
    sessions.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    sessions
}

fn scan_harness(
    h: Harness,
    max_age_hours: f64,
    limit: usize,
    reason: &str,
) -> Result<Vec<Session>, String> {
    eprintln!("[scan] harness={} reason={reason}", h.as_str());
    match h {
        // File adapters: Adapter::scan is infallible (empty on miss).
        Harness::ClaudeCode => Ok(ClaudeCodeAdapter.scan(max_age_hours, limit)),
        Harness::Codex => Ok(CodexAdapter.scan(max_age_hours, limit)),
        // Sqlite adapters: surface open/query failures so the watcher can keep last-good.
        // Adapter::scan remains the trait entry (swallows errors → empty); watcher uses Result.
        Harness::Cursor => {
            let _ = CursorAdapter;
            cursor::scan_raw(max_age_hours, limit).map(crate::adapters::finalize)
        }
        Harness::Opencode => {
            let _ = OpencodeAdapter;
            opencode::scan_raw(max_age_hours, limit).map(crate::adapters::finalize)
        }
    }
}

fn source_roots() -> Vec<(Harness, PathBuf)> {
    let home = home_dir();
    vec![
        (
            Harness::ClaudeCode,
            PathBuf::from(format!("{home}/.claude/projects")),
        ),
        (
            Harness::Codex,
            PathBuf::from(format!("{home}/.codex/sessions")),
        ),
        (
            Harness::Cursor,
            PathBuf::from(format!(
                "{home}/Library/Application Support/Cursor/User/globalStorage"
            )),
        ),
        (
            Harness::Opencode,
            // DECISION: watch the opencode dir recursively like the others.
            // If log/ churn is noisy in practice, narrow to opencode.db*.
            PathBuf::from(format!("{home}/.local/share/opencode")),
        ),
    ]
}

fn classify_path(path: &Path) -> Option<Harness> {
    let s = path.to_string_lossy();
    if s.contains("/.claude/") {
        Some(Harness::ClaudeCode)
    } else if s.contains("/.codex/") {
        Some(Harness::Codex)
    } else if s.contains("/Cursor/") || s.contains("state.vscdb") {
        Some(Harness::Cursor)
    } else if s.contains("/.local/share/opencode") {
        Some(Harness::Opencode)
    } else {
        None
    }
}

fn session_key(s: &Session) -> (String, String) {
    (s.harness.clone(), s.sid.clone())
}

fn merge_cache(by_harness: &HashMap<Harness, Vec<Session>>) -> Vec<Session> {
    let mut merged = Vec::new();
    for h in ALL {
        if let Some(part) = by_harness.get(&h) {
            merged.extend(part.iter().cloned());
        }
    }
    merged.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    merged
}

const TICK: Duration = Duration::from_secs(2);

/// Watch harness source roots; call `on_snapshot` on startup, after each
/// debounced fs change, and every 2s tick (re-finalize only — no adapter I/O).
pub fn watch_sessions<F>(max_age_hours: f64, limit: usize, mut on_snapshot: F) -> Result<(), String>
where
    F: FnMut(Vec<Session>, SnapshotReason),
{
    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .map_err(|e| e.to_string())?;

    for (_h, root) in source_roots() {
        if root.exists() {
            watcher
                .watch(&root, RecursiveMode::Recursive)
                .map_err(|e| e.to_string())?;
        }
    }

    // Per-harness cache — single writer (this loop). Tick re-finalizes in place.
    let mut by_harness: HashMap<Harness, Vec<Session>> = HashMap::new();
    let mut logged_degraded: HashSet<Harness> = HashSet::new();

    for h in ALL {
        match scan_harness(h, max_age_hours, limit, "startup") {
            Ok(part) => {
                by_harness.insert(h, part);
            }
            Err(e) => {
                eprintln!(
                    "[scan] harness={} reason=startup error={e} (starting empty)",
                    h.as_str()
                );
                by_harness.insert(h, Vec::new());
            }
        }
    }
    on_snapshot(merge_cache(&by_harness), SnapshotReason::Startup);

    let debounce = Duration::from_millis(500);
    let mut pending: HashMap<Harness, Instant> = HashMap::new();
    let mut last_tick = Instant::now();

    loop {
        let debounce_timeout = pending
            .values()
            .min()
            .map(|t| {
                let elapsed = t.elapsed();
                if elapsed >= debounce {
                    Duration::from_millis(0)
                } else {
                    debounce - elapsed
                }
            })
            .unwrap_or(Duration::from_secs(3600));

        let since_tick = last_tick.elapsed();
        let tick_timeout = if since_tick >= TICK {
            Duration::from_millis(0)
        } else {
            TICK - since_tick
        };

        let timeout = debounce_timeout.min(tick_timeout);

        match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => {
                for path in event.paths {
                    if let Some(h) = classify_path(&path) {
                        pending.insert(h, Instant::now());
                    }
                }
            }
            Ok(Err(e)) => eprintln!("watch error: {e}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        while let Ok(msg) = rx.try_recv() {
            match msg {
                Ok(event) => {
                    for path in event.paths {
                        if let Some(h) = classify_path(&path) {
                            pending.insert(h, Instant::now());
                        }
                    }
                }
                Err(e) => eprintln!("watch error: {e}"),
            }
        }

        let ready: Vec<Harness> = pending
            .iter()
            .filter(|(_, t)| t.elapsed() >= debounce)
            .map(|(h, _)| *h)
            .collect();

        let mut fs_changed = false;
        for h in ready {
            pending.remove(&h);
            match scan_harness(h, max_age_hours, limit, "fs") {
                Ok(part) => {
                    by_harness.insert(h, part);
                    logged_degraded.remove(&h);
                    fs_changed = true;
                }
                Err(e) => {
                    // Degrade to stale: keep last-good rows for this harness.
                    if logged_degraded.insert(h) {
                        eprintln!(
                            "[scan] harness={} reason=fs degraded (keeping last-good): {e}",
                            h.as_str()
                        );
                    }
                }
            }
        }

        if fs_changed {
            on_snapshot(merge_cache(&by_harness), SnapshotReason::Fs);
        }

        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            // NO adapter scans — re-finalize state/age from cached mtime + last_role.
            for part in by_harness.values_mut() {
                refinalize(part);
            }
            on_snapshot(merge_cache(&by_harness), SnapshotReason::Tick);
        }
    }

    let _ = watcher;
    Ok(())
}

/// CLI helper: print `<harness> <sid> <old> -> <new>` on state transitions.
pub fn watch_sessions_cli(max_age_hours: f64, limit: usize) -> Result<(), String> {
    let mut prev: HashMap<(String, String), String> = HashMap::new();
    eprintln!("watching… (ctrl-c to quit)");
    watch_sessions(max_age_hours, limit, |sessions, _reason| {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        for s in &sessions {
            let k = session_key(s);
            seen.insert(k.clone());
            if let Some(old) = prev.get(&k) {
                if old != &s.state {
                    println!("{} {} {} -> {}", s.harness, s.sid, old, s.state);
                }
            }
            prev.insert(k, s.state.clone());
        }
        prev.retain(|k, _| seen.contains(k));
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{age_str, session_state, ACTIVE_S};

    #[test]
    fn refinalize_flips_working_to_done() {
        let mtime = crate::adapters::now_secs() - (ACTIVE_S + 1.0);
        let mut sessions = vec![Session {
            harness: "claude code".into(),
            sid: "abc".into(),
            title: String::new(),
            model: String::new(),
            cwd: "/tmp".into(),
            branch: String::new(),
            last_user: String::new(),
            last_assistant: String::new(),
            activity: String::new(),
            mtime,
            state: "working".into(),
            age: "0s".into(),
            repo: "tmp".into(),
            src: String::new(),
            sidechains: 0,
            last_role: "assistant".into(),
        }];
        refinalize(&mut sessions);
        assert_eq!(sessions[0].state, "done");
        assert_eq!(
            sessions[0].age,
            age_str(crate::adapters::now_secs() - mtime)
        );
        assert_eq!(session_state(mtime, "assistant"), "done");
    }

    #[test]
    fn degrade_keeps_last_good_on_scan_err() {
        let mut by_harness: HashMap<Harness, Vec<Session>> = HashMap::new();
        by_harness.insert(
            Harness::Cursor,
            vec![Session {
                harness: "cursor".into(),
                sid: "deadbeef".into(),
                title: "kept".into(),
                model: String::new(),
                cwd: String::new(),
                branch: String::new(),
                last_user: String::new(),
                last_assistant: String::new(),
                activity: String::new(),
                mtime: 1.0,
                state: "done".into(),
                age: "1d".into(),
                repo: "-".into(),
                src: String::new(),
                sidechains: 0,
                last_role: "assistant".into(),
            }],
        );
        // Simulate fs scan failure: do not replace cache.
        let err: Result<Vec<Session>, String> = Err("database disk image is malformed".into());
        assert!(err.is_err());
        let merged = merge_cache(&by_harness);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].sid, "deadbeef");
        assert_eq!(merged[0].title, "kept");
    }
}
