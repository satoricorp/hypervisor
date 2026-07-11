use super::*;
use glob::glob;
use serde_json::Value;
use std::path::PathBuf;

pub struct CodexAdapter;

impl Adapter for CodexAdapter {
    fn scan(&self, max_age_hours: f64, limit: usize) -> Vec<Session> {
        finalize(scan_raw(max_age_hours, limit))
    }
}

pub fn scan_raw(max_age_hours: f64, limit: usize) -> Vec<RawSession> {
    let pattern = format!("{}/.codex/sessions/*/*/*/rollout-*.jsonl", home_dir());
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
        // spike: os.path.basename(path)[:-6][-8:]  — strip .jsonl then last 8 chars
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let sid = if stem.len() >= 8 {
            stem[stem.len() - 8..].to_string()
        } else {
            stem
        };
        let src = path.to_string_lossy().to_string();
        let mut s = empty_raw("codex", &sid, mtime, &src);
        let mut first_user = String::new();

        for line in read_lines(&path) {
            let e = match parse_json_object(&line) {
                Some(v) => v,
                None => continue,
            };
            let typ = e.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let p = e
                .get("payload")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            if typ == "session_meta" {
                if let Some(cwd) = p.get("cwd").and_then(|v| v.as_str()) {
                    if !cwd.is_empty() {
                        s.cwd = cwd.to_string();
                    }
                }
                if let Some(git) = p.get("git") {
                    if let Some(branch) = git.get("branch").and_then(|v| v.as_str()) {
                        if !branch.is_empty() {
                            s.branch = branch.to_string();
                        }
                    }
                }
            } else if typ == "turn_context" {
                if let Some(model) = p.get("model").and_then(|v| v.as_str()) {
                    if !model.is_empty() {
                        s.model = model.to_string();
                    }
                }
                if let Some(cwd) = p.get("cwd").and_then(|v| v.as_str()) {
                    if !cwd.is_empty() {
                        s.cwd = cwd.to_string();
                    }
                }
            } else if typ == "response_item" {
                let pt = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if pt == "message" {
                    let texts: Vec<String> = p
                        .get("content")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|c| {
                                    let t = c.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                    if t == "input_text" || t == "output_text" {
                                        c.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let text = texts.into_iter().find(|t| !is_noise(t)).unwrap_or_default();
                    if text.is_empty() {
                        continue;
                    }
                    if p.get("role").and_then(|v| v.as_str()) == Some("user") {
                        s.last_user = text.clone();
                        s.last_role = "user".into();
                        if first_user.is_empty() {
                            first_user = text;
                        }
                    } else {
                        s.last_assistant = text;
                        s.last_role = "assistant".into();
                    }
                } else if pt == "function_call"
                    || pt == "local_shell_call"
                    || pt == "custom_tool_call"
                {
                    let name = p
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(pt)
                        .to_string();
                    let arg = if let Some(a) = p.get("arguments") {
                        match a {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        }
                    } else if let Some(action) = p.get("action") {
                        action.to_string()
                    } else {
                        String::new()
                    };
                    s.activity = format!("⚒ {name}({})", clip(&arg, 46));
                    s.last_role = "assistant".into();
                } else if pt == "reasoning" {
                    if let Some(summary) = p.get("summary").and_then(|v| v.as_array()) {
                        for sm in summary {
                            if let Some(text) = sm.get("text").and_then(|v| v.as_str()) {
                                if !text.is_empty() {
                                    s.last_assistant = text.to_string();
                                }
                            }
                        }
                    }
                }
            } else if typ == "event_msg" && p.get("type").and_then(|v| v.as_str()) == Some("agent_message")
            {
                if let Some(msg) = p.get("message").and_then(|v| v.as_str()) {
                    if !msg.is_empty() {
                        s.last_assistant = msg.to_string();
                        s.last_role = "assistant".into();
                    }
                }
            }
        }
        // codex session_meta carries no title (checked 2026-07-10) — derive
        s.title = derive_title(&first_user);
        if !s.title.is_empty() {
            out.push(s);
        }
    }
    out.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    out
}
