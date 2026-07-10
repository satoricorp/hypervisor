//! Persist `{ sid → tmux_session_name }` in app_data_dir/owned.json.
//! Correlate a freshly spawned tmux session with its transcript file.

use crate::adapters::{file_mtime, home_dir};
use chrono::Local;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub type OwnedMap = HashMap<String, String>;

pub fn load(path: &Path) -> OwnedMap {
    let data = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

pub fn save(path: &Path, map: &OwnedMap) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())
}

fn munge_cwd(cwd: &str) -> String {
    cwd.replace('/', "-")
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Poll for a new transcript file created after `spawn_time`. Returns sid.
pub fn wait_for_sid(harness: &str, cwd: &str, spawn_time: f64) -> Option<String> {
    let deadline = now_secs() + 15.0;
    while now_secs() < deadline {
        if let Some(sid) = find_new_sid(harness, cwd, spawn_time) {
            return Some(sid);
        }
        thread::sleep(Duration::from_millis(500));
    }
    None
}

fn find_new_sid(harness: &str, cwd: &str, spawn_time: f64) -> Option<String> {
    match harness {
        "claude" | "claude code" => find_claude_sid(cwd, spawn_time),
        "codex" => find_codex_sid(spawn_time),
        _ => None,
    }
}

fn find_claude_sid(cwd: &str, spawn_time: f64) -> Option<String> {
    let dir = PathBuf::from(format!(
        "{}/.claude/projects/{}",
        home_dir(),
        munge_cwd(cwd)
    ));
    newest_jsonl_sid(&dir, spawn_time, |stem| stem.to_string())
}

fn find_codex_sid(spawn_time: f64) -> Option<String> {
    // DECISION: adapter sid is last 8 chars of stem (not full basename), so
    // owned.json keys match sidebar rows from hvscan/adapters.
    let today = Local::now().format("%Y/%m/%d").to_string();
    let dir = PathBuf::from(format!("{}/.codex/sessions/{today}", home_dir()));
    newest_jsonl_sid(&dir, spawn_time, |stem| {
        if stem.len() >= 8 {
            stem[stem.len() - 8..].to_string()
        } else {
            stem.to_string()
        }
    })
}

fn newest_jsonl_sid<F>(dir: &Path, spawn_time: f64, sid_from_stem: F) -> Option<String>
where
    F: Fn(&str) -> String,
{
    let entries = fs::read_dir(dir).ok()?;
    let mut best: Option<(f64, String)> = None;
    for ent in entries.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let mtime = match file_mtime(&path) {
            Some(m) => m,
            None => continue,
        };
        // Allow a small clock skew; file must be at/after spawn.
        if mtime + 1.0 < spawn_time {
            continue;
        }
        let stem = path.file_stem()?.to_str()?;
        let sid = sid_from_stem(stem);
        if best.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
            best = Some((mtime, sid));
        }
    }
    best.map(|(_, sid)| sid)
}
