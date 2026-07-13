use super::*;
use glob::glob;
use serde_json::Value;
use std::path::PathBuf;

pub struct ClaudeCodeAdapter;

impl Adapter for ClaudeCodeAdapter {
    fn scan(&self, max_age_hours: f64, limit: usize) -> Vec<Session> {
        finalize(scan_raw(max_age_hours, limit))
    }
}

pub fn scan_raw(max_age_hours: f64, limit: usize) -> Vec<RawSession> {
    let pattern = format!("{}/.claude/projects/*/*.jsonl", home_dir());
    let mut out = Vec::new();
    let now = now_secs();
    let paths: Vec<PathBuf> = match glob(&pattern) {
        Ok(g) => g.filter_map(|r| r.ok()).collect(),
        Err(_) => return out,
    };
    for path in paths {
        let mtime = match file_mtime(&path) {
            Some(m) => m,
            None => continue,
        };
        if now - mtime > max_age_hours * 3600.0 {
            continue;
        }
        let sid = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let src = path.to_string_lossy().to_string();
        let mut s = empty_raw("claude code", &sid, mtime, &src);
        let mut first_user = String::new();
        let mut summary_title = String::new();

        for line in read_lines(&path) {
            let e = match parse_json_object(&line) {
                Some(v) => v,
                None => continue,
            };
            if e.get("isSidechain").and_then(|v| v.as_bool()).unwrap_or(false) {
                let typ = e.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let parent = e.get("parentUuid");
                let no_parent = parent.is_none() || parent == Some(&Value::Null);
                if typ == "user" && no_parent {
                    s.sidechains += 1;
                }
                continue;
            }
            if let Some(cwd) = e.get("cwd").and_then(|v| v.as_str()) {
                if !cwd.is_empty() {
                    s.cwd = cwd.to_string();
                }
            }
            if let Some(branch) = e.get("gitBranch").and_then(|v| v.as_str()) {
                if !branch.is_empty() {
                    s.branch = branch.to_string();
                }
            }
            if let Some(ep) = e.get("entrypoint").and_then(|v| v.as_str()) {
                if !ep.is_empty() {
                    s.entrypoint = ep.to_string();
                }
            }
            let typ = e.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let msg = e.get("message").cloned().unwrap_or(Value::Object(Default::default()));
            let content = msg.get("content");
            if typ == "summary" {
                // claude code's own generated short title (written on
                // compaction) — prefer the latest one when present
                if let Some(t) = e.get("summary").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        summary_title = t.to_string();
                    }
                }
            } else if typ == "user" && !e.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
                let texts = extract_user_texts(content);
                for t in texts {
                    if !is_noise(&t) {
                        s.last_user = t.clone();
                        s.last_role = "user".into();
                        if first_user.is_empty() {
                            first_user = t;
                        }
                    }
                }
            } else if typ == "assistant" {
                if let Some(model) = msg.get("model").and_then(|v| v.as_str()) {
                    if !model.is_empty() {
                        s.model = model.to_string();
                    }
                }
                if let Some(Value::Array(blocks)) = content {
                    for c in blocks {
                        if !c.is_object() {
                            continue;
                        }
                        let ctype = c.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if ctype == "tool_use" {
                            let arg = c.get("input").cloned().unwrap_or(Value::Object(Default::default()));
                            let hint = tool_hint(&arg);
                            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            s.activity = format!("⚒ {name}({})", clip(&hint, 46));
                            s.last_role = "assistant".into();
                        } else if ctype == "text" {
                            if let Some(text) = c.get("text").and_then(|v| v.as_str()) {
                                if !text.is_empty() {
                                    s.last_assistant = text.to_string();
                                    s.last_role = "assistant".into();
                                }
                            }
                        }
                    }
                }
            }
        }
        s.title = if !summary_title.is_empty() {
            clip(&summary_title, 64)
        } else {
            derive_title(&first_user)
        };
        if !s.title.is_empty() {
            out.push(s);
        }
    }
    out.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    out
}

fn extract_user_texts(content: Option<&Value>) -> Vec<String> {
    match content {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|c| {
                if c.get("type").and_then(|v| v.as_str()) == Some("text") {
                    c.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn tool_hint(arg: &Value) -> String {
    if let Some(fp) = arg.get("file_path").and_then(|v| v.as_str()) {
        if !fp.is_empty() {
            return fp.to_string();
        }
    }
    if let Some(p) = arg.get("path").and_then(|v| v.as_str()) {
        if !p.is_empty() {
            return p.to_string();
        }
    }
    if let Some(cmd) = arg.get("command") {
        let s = match cmd {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !s.is_empty() && s != "null" {
            return clip(&s, 40);
        }
    }
    if let Some(pat) = arg.get("pattern") {
        let s = match pat {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !s.is_empty() && s != "null" {
            return clip(&s, 40);
        }
    }
    String::new()
}
