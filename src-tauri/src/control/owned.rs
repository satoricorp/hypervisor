//! Persist `{ sid → { tmux, harness } }` in app_data_dir/owned.json.
//! Correlate a freshly spawned tmux session with its transcript file.
//!
//! v2 values are objects; load() still accepts legacy plain-string values
//! (harness unknown → empty → detect_tmux falls back to snapshot lookup).

use crate::adapters::{file_mtime, home_dir};
use crate::control::tmux;
use crate::registry::{scan_sessions, Harness};
use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

/// M4: the git worktree a spawned session runs in (when isolated). Persisted so
/// the repo·branch·worktree header survives a restart without re-shelling git.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Worktree {
    /// Shared repo label (header `repo`), e.g. `hypervisor`.
    pub repo: String,
    /// Dedicated branch, e.g. `hv-1a2b3c4d`.
    pub branch: String,
    /// Worktree directory (the session's cwd).
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnedEntry {
    pub tmux: String,
    pub harness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Worktree>,
}

impl OwnedEntry {
    pub fn new(tmux: impl Into<String>, harness: impl Into<String>) -> Self {
        Self {
            tmux: tmux.into(),
            harness: harness.into(),
            worktree: None,
        }
    }

    pub fn with_worktree(mut self, worktree: Option<Worktree>) -> Self {
        self.worktree = worktree;
        self
    }
}

pub type OwnedMap = HashMap<String, OwnedEntry>;

pub fn load(path: &Path) -> OwnedMap {
    load_with(path, |name| tmux::has_session(name))
}

/// Load + migrate + prune. `alive` decides whether a tmux name still exists.
pub fn load_with(path: &Path, alive: impl Fn(&str) -> bool) -> OwnedMap {
    let data = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    let raw: HashMap<String, serde_json::Value> = match serde_json::from_str(&data) {
        Ok(m) => m,
        Err(_) => return HashMap::new(),
    };
    let mut map = OwnedMap::new();
    let mut dirty = false;
    for (sid, val) in raw {
        let entry = match parse_entry(&val) {
            Some(e) => e,
            None => {
                dirty = true;
                continue;
            }
        };
        // Legacy string form → rewrite as v2 on next save.
        if val.is_string() {
            dirty = true;
        }
        if !alive(&entry.tmux) {
            dirty = true;
            continue;
        }
        map.insert(sid, entry);
    }
    if dirty {
        let _ = save(path, &map);
    }
    map
}

fn parse_entry(val: &serde_json::Value) -> Option<OwnedEntry> {
    match val {
        serde_json::Value::String(tmux) if !tmux.is_empty() => {
            Some(OwnedEntry::new(tmux.clone(), ""))
        }
        serde_json::Value::Object(obj) => {
            let tmux = obj.get("tmux")?.as_str()?.to_string();
            if tmux.is_empty() {
                return None;
            }
            let harness = obj
                .get("harness")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let worktree = obj
                .get("worktree")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            Some(OwnedEntry {
                tmux,
                harness,
                worktree,
            })
        }
        _ => None,
    }
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
        thread::sleep(StdDuration::from_millis(500));
    }
    None
}

fn find_new_sid(harness: &str, cwd: &str, spawn_time: f64) -> Option<String> {
    match harness {
        "claude" | "claude code" => find_claude_sid(cwd, spawn_time),
        "codex" => find_codex_sid(spawn_time),
        "opencode" => find_opencode_sid(cwd, spawn_time),
        _ => None,
    }
}

fn find_opencode_sid(cwd: &str, spawn_time: f64) -> Option<String> {
    // DECISION: Session has no time_created field; for brand-new sessions
    // mtime (time_updated/1000) equals time_created at creation, so the
    // spawn_time floor still correlates correctly.
    let sessions = scan_sessions(48.0, 32, Some(Harness::Opencode));
    sessions
        .into_iter()
        .filter(|s| s.cwd == cwd && s.mtime + 1.0 >= spawn_time)
        .max_by(|a, b| {
            a.mtime
                .partial_cmp(&b.mtime)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|s| s.sid)
}

fn find_claude_sid(cwd: &str, spawn_time: f64) -> Option<String> {
    let dir = PathBuf::from(format!(
        "{}/.claude/projects/{}",
        home_dir(),
        munge_cwd(cwd)
    ));
    newest_jsonl_sid(&dir, spawn_time, |stem| stem.to_string()).map(|(_, sid)| sid)
}

fn find_codex_sid(spawn_time: f64) -> Option<String> {
    let base = PathBuf::from(format!("{}/.codex/sessions", home_dir()));
    find_codex_sid_in(&base, spawn_time, Local::now().date_naive())
}

/// Scan today's and yesterday's date dirs; pick the newest matching jsonl.
fn find_codex_sid_in(base: &Path, spawn_time: f64, today: NaiveDate) -> Option<String> {
    // DECISION: adapter sid is last 8 chars of stem (not full basename), so
    // owned.json keys match sidebar rows from hvscan/adapters.
    let yesterday = today - Duration::days(1);
    let mut best: Option<(f64, String)> = None;
    for day in [today, yesterday] {
        let dir = base.join(day.format("%Y/%m/%d").to_string());
        if let Some((mtime, sid)) = newest_jsonl_sid(&dir, spawn_time, |stem| {
            if stem.len() >= 8 {
                stem[stem.len() - 8..].to_string()
            } else {
                stem.to_string()
            }
        }) {
            if best.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
                best = Some((mtime, sid));
            }
        }
    }
    best.map(|(_, sid)| sid)
}

fn newest_jsonl_sid<F>(dir: &Path, spawn_time: f64, sid_from_stem: F) -> Option<(f64, String)>
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
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_migrates_legacy_string_and_prunes_dead() {
        let dir = std::env::temp_dir().join(format!("hv-owned-{}", short_tmp()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("owned.json");
        fs::write(
            &path,
            r#"{
  "dead-sid": "hv-dead",
  "live-sid": "hv-live",
  "v2-sid": {"tmux": "hv-v2", "harness": "claude code"}
}"#,
        )
        .unwrap();

        let map = load_with(&path, |name| name == "hv-live" || name == "hv-v2");
        assert!(!map.contains_key("dead-sid"), "dead tmux pruned");
        let live = map.get("live-sid").expect("legacy live kept");
        assert_eq!(live.tmux, "hv-live");
        assert_eq!(live.harness, "", "legacy harness unknown");
        let v2 = map.get("v2-sid").expect("v2 kept");
        assert_eq!(v2.tmux, "hv-v2");
        assert_eq!(v2.harness, "claude code");

        // Saved as v2 (no plain strings).
        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(saved["live-sid"].is_object(), "legacy rewritten as object");
        assert_eq!(saved["live-sid"]["tmux"], "hv-live");
        assert_eq!(saved["live-sid"]["harness"], "");
        assert!(saved.get("dead-sid").is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_codex_sid_scans_today_and_yesterday() {
        let base = std::env::temp_dir().join(format!("hv-codex-{}", short_tmp()));
        let today = NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
        let ydir = base.join("2026/07/09");
        let tdir = base.join("2026/07/10");
        fs::create_dir_all(&ydir).unwrap();
        fs::create_dir_all(&tdir).unwrap();

        // Only yesterday has a file — must still correlate across midnight.
        let ypath = ydir.join(
            "rollout-2026-07-09T23-50-00-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.jsonl",
        );
        write_jsonl(&ypath);
        let sid = find_codex_sid_in(&base, 0.0, today).expect("yesterday sid");
        assert_eq!(sid, "eeeeeeee");

        // Today is newer on disk → wins over yesterday.
        thread::sleep(StdDuration::from_millis(20));
        let tpath = tdir.join(
            "rollout-2026-07-10T01-00-00-11111111-2222-3333-4444-555555555555.jsonl",
        );
        write_jsonl(&tpath);
        let sid = find_codex_sid_in(&base, 0.0, today).expect("today sid");
        assert_eq!(sid, "55555555");

        let _ = fs::remove_dir_all(&base);
    }

    fn short_tmp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    fn write_jsonl(path: &Path) {
        let mut f = fs::File::create(path).unwrap();
        writeln!(f, "{{}}").unwrap();
    }
}
