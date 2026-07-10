pub mod claude_code;
pub mod codex;
pub mod cursor;
pub mod opencode;

use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const ACTIVE_S: f64 = 15.0;
pub const STALL_S: f64 = 90.0;
pub const TAIL_BYTES: u64 = 512 * 1024;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Session {
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
    /// Kept for tick-time re-finalize (working→done) without rescanning.
    #[serde(skip)]
    pub last_role: String,
}

/// Internal scan row before finalize fills state/age/repo.
#[derive(Clone, Debug)]
pub struct RawSession {
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
    pub src: String,
    pub last_role: String,
    pub sidechains: u32,
}

pub trait Adapter {
    fn scan(&self, max_age_hours: f64, limit: usize) -> Vec<Session>;
}

pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}

pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn file_mtime(path: &Path) -> Option<f64> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    Some(
        modified
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0),
    )
}

pub fn clip(s: &str, n: usize) -> String {
    let collapsed: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= n {
        collapsed
    } else {
        let truncated: String = collapsed.chars().take(n.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

pub fn age_str(secs: f64) -> String {
    if secs < 60.0 {
        format!("{}s", secs as i64)
    } else if secs < 3600.0 {
        format!("{}m", (secs / 60.0) as i64)
    } else if secs < 86400.0 {
        format!("{}h", (secs / 3600.0) as i64)
    } else {
        format!("{}d", (secs / 86400.0) as i64)
    }
}

pub fn is_noise(text: &str) -> bool {
    let t = text.trim_start();
    t.is_empty()
        || t.starts_with('<')
        || t.starts_with("Caveat:")
        || t.starts_with("# AGENTS.md")
        || t.chars().take(400).collect::<String>().contains("<INSTRUCTIONS>")
}

pub fn session_state(mtime: f64, last_role: &str) -> String {
    let idle = now_secs() - mtime;
    if idle <= ACTIVE_S {
        "working".into()
    } else if last_role == "user" && idle > STALL_S {
        "stalled".into()
    } else {
        "done".into()
    }
}

pub fn read_lines(path: &Path) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return Vec::new(),
    };
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let data = if size <= 2 * TAIL_BYTES {
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        buf
    } else {
        let mut head = vec![0u8; TAIL_BYTES as usize];
        if file.read_exact(&mut head).is_err() {
            return Vec::new();
        }
        if file.seek(SeekFrom::Start(size - TAIL_BYTES)).is_err() {
            return Vec::new();
        }
        let mut tail = Vec::new();
        if file.read_to_end(&mut tail).is_err() {
            return Vec::new();
        }
        // drop the partial line at the start of the tail chunk
        let skip = match tail.iter().position(|&b| b == b'\n') {
            Some(i) => i + 1,
            None => 0,
        };
        let mut data = head;
        data.push(b'\n');
        data.extend_from_slice(&tail[skip..]);
        data
    };
    String::from_utf8_lossy(&data)
        .lines()
        .map(|l| l.to_string())
        .collect()
}

pub fn parse_json_object(line: &str) -> Option<serde_json::Value> {
    match serde_json::from_str::<serde_json::Value>(line) {
        Ok(v) if v.is_object() => Some(v),
        _ => None,
    }
}

pub fn finalize(raw: Vec<RawSession>) -> Vec<Session> {
    let now = now_secs();
    raw.into_iter()
        .map(|s| {
            let repo = {
                let base = Path::new(&s.cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if base.is_empty() {
                    "-".into()
                } else {
                    base.to_string()
                }
            };
            Session {
                harness: s.harness,
                sid: s.sid,
                title: s.title,
                model: s.model,
                cwd: s.cwd,
                branch: s.branch,
                last_user: s.last_user,
                last_assistant: s.last_assistant,
                activity: s.activity,
                mtime: s.mtime,
                state: session_state(s.mtime, &s.last_role),
                age: age_str(now - s.mtime),
                repo,
                src: s.src,
                sidechains: s.sidechains,
                last_role: s.last_role,
            }
        })
        .collect()
}

/// Recompute state/age from cached mtime + last_role (no disk I/O).
pub fn refinalize(sessions: &mut [Session]) {
    let now = now_secs();
    for s in sessions {
        s.state = session_state(s.mtime, &s.last_role);
        s.age = age_str(now - s.mtime);
    }
}

pub fn empty_raw(harness: &str, sid: &str, mtime: f64, src: &str) -> RawSession {
    RawSession {
        harness: harness.into(),
        sid: sid.into(),
        title: String::new(),
        model: String::new(),
        cwd: String::new(),
        branch: String::new(),
        last_user: String::new(),
        last_assistant: String::new(),
        activity: String::new(),
        mtime,
        src: src.into(),
        last_role: String::new(),
        sidechains: 0,
    }
}
