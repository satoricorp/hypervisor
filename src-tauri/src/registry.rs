use crate::adapters::claude_code::ClaudeCodeAdapter;
use crate::adapters::codex::CodexAdapter;
use crate::adapters::cursor::CursorAdapter;
use crate::adapters::{home_dir, Adapter, Session};
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
}

impl Harness {
    pub fn as_str(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude code",
            Harness::Codex => "codex",
            Harness::Cursor => "cursor",
        }
    }
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
        None => vec![Harness::ClaudeCode, Harness::Codex, Harness::Cursor],
    };
    for h in harnesses {
        let part = match h {
            Harness::ClaudeCode => ClaudeCodeAdapter.scan(max_age_hours, limit),
            Harness::Codex => CodexAdapter.scan(max_age_hours, limit),
            Harness::Cursor => CursorAdapter.scan(max_age_hours, limit),
        };
        sessions.extend(part);
    }
    sessions.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    sessions
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
    } else {
        None
    }
}

fn session_key(s: &Session) -> (String, String) {
    (s.harness.clone(), s.sid.clone())
}

/// Watch harness source roots; call `on_snapshot` with the full merged list
/// on startup and after each debounced change (500ms).
pub fn watch_sessions<F>(max_age_hours: f64, limit: usize, mut on_snapshot: F) -> Result<(), String>
where
    F: FnMut(Vec<Session>),
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

    // Initial full snapshot before any fs event.
    on_snapshot(scan_sessions(max_age_hours, limit, None));

    let debounce = Duration::from_millis(500);
    let mut pending: HashMap<Harness, Instant> = HashMap::new();
    // Cache per-harness so we only rescan the harness that changed.
    let mut by_harness: HashMap<Harness, Vec<Session>> = HashMap::new();
    for h in [Harness::ClaudeCode, Harness::Codex, Harness::Cursor] {
        by_harness.insert(h, scan_sessions(max_age_hours, limit, Some(h)));
    }

    loop {
        let timeout = pending
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

        if ready.is_empty() {
            continue;
        }

        for h in ready {
            pending.remove(&h);
            by_harness.insert(h, scan_sessions(max_age_hours, limit, Some(h)));
        }

        let mut merged = Vec::new();
        for h in [Harness::ClaudeCode, Harness::Codex, Harness::Cursor] {
            if let Some(part) = by_harness.get(&h) {
                merged.extend(part.iter().cloned());
            }
        }
        merged.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
        on_snapshot(merged);
    }

    let _ = watcher;
    Ok(())
}

/// CLI helper: print `<harness> <sid> <old> -> <new>` on state transitions.
pub fn watch_sessions_cli(max_age_hours: f64, limit: usize) -> Result<(), String> {
    let mut prev: HashMap<(String, String), String> = HashMap::new();
    eprintln!("watching… (ctrl-c to quit)");
    watch_sessions(max_age_hours, limit, |sessions| {
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
