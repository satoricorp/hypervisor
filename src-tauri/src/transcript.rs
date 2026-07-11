//! On-demand transcript parse for the selected session.
//! NOT used by the snapshot hot loop — keep registry::watch_sessions untouched.

use crate::adapters::{clip, is_noise, parse_json_object, read_lines};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptItem {
    User {
        text: String,
    },
    Assistant {
        text: String,
    },
    Thinking {
        text: String,
    },
    Tool {
        id: String,
        name: String,
        /// Short hint for the collapsed one-liner (command / path / pattern).
        summary: String,
        /// Pretty-printed tool input JSON.
        input: String,
        result: Option<String>,
        is_error: bool,
    },
}

/// Parse a session source file into typed transcript items.
/// `limit` caps the returned tail (0 = default 400).
pub fn parse_transcript(src: &str, harness: &str, limit: usize) -> Vec<TranscriptItem> {
    let path = Path::new(src);
    if !path.exists() {
        return Vec::new();
    }
    let limit = if limit == 0 { 400 } else { limit };
    let items = if harness == "claude code" || src.contains("/.claude/projects/") {
        parse_claude(path)
    } else if harness == "codex" || src.contains("/.codex/") {
        parse_codex_best_effort(path)
    } else {
        // cursor / opencode / unknown — leave out (no verified JSONL shape here)
        Vec::new()
    };
    if items.len() <= limit {
        items
    } else {
        items[items.len() - limit..].to_vec()
    }
}

fn parse_claude(path: &Path) -> Vec<TranscriptItem> {
    let mut items: Vec<TranscriptItem> = Vec::new();
    // tool_use id → index in items
    let mut tool_idx: HashMap<String, usize> = HashMap::new();

    for line in read_lines(path) {
        let e = match parse_json_object(&line) {
            Some(v) => v,
            None => continue,
        };
        if e.get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let typ = e.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let msg = e
            .get("message")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));
        let content = msg.get("content");

        if typ == "user" && !e.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
            match content {
                Some(Value::String(s)) => {
                    if !is_noise(s) {
                        items.push(TranscriptItem::User { text: s.clone() });
                    }
                }
                Some(Value::Array(arr)) => {
                    for c in arr {
                        let ctype = c.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if ctype == "tool_result" {
                            apply_tool_result(&mut items, &mut tool_idx, c);
                        } else if ctype == "text" {
                            if let Some(text) = c.get("text").and_then(|v| v.as_str()) {
                                if !is_noise(text) {
                                    items.push(TranscriptItem::User {
                                        text: text.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        } else if typ == "assistant" {
            if let Some(Value::Array(blocks)) = content {
                for c in blocks {
                    if !c.is_object() {
                        continue;
                    }
                    let ctype = c.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if ctype == "thinking" {
                        // Only emit when the field has real text — empty
                        // thinking+signature blobs are noise (verified empty
                        // across local ~/.claude/projects).
                        if let Some(t) = c.get("thinking").and_then(|v| v.as_str()) {
                            let t = t.trim();
                            if !t.is_empty() {
                                items.push(TranscriptItem::Thinking {
                                    text: t.to_string(),
                                });
                            }
                        }
                    } else if ctype == "text" {
                        if let Some(text) = c.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                items.push(TranscriptItem::Assistant {
                                    text: text.to_string(),
                                });
                            }
                        }
                    } else if ctype == "tool_use" {
                        let id = c
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = c
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tool")
                            .to_string();
                        let input_val = c
                            .get("input")
                            .cloned()
                            .unwrap_or(Value::Object(Default::default()));
                        let summary = tool_summary(&input_val);
                        let input = pretty_input(&input_val);
                        if !id.is_empty() {
                            tool_idx.insert(id.clone(), items.len());
                        }
                        items.push(TranscriptItem::Tool {
                            id,
                            name,
                            summary,
                            input,
                            result: None,
                            is_error: false,
                        });
                    }
                }
            }
        }
    }
    items
}

fn apply_tool_result(
    items: &mut [TranscriptItem],
    tool_idx: &mut HashMap<String, usize>,
    block: &Value,
) {
    let id = block
        .get("tool_use_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if id.is_empty() {
        return;
    }
    let is_error = block
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let result = flatten_tool_result(block.get("content"));
    if let Some(&idx) = tool_idx.get(id) {
        if let Some(TranscriptItem::Tool {
            result: r,
            is_error: err,
            ..
        }) = items.get_mut(idx)
        {
            *r = Some(result);
            *err = is_error;
        }
    }
}

fn flatten_tool_result(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|c| {
                if c.get("type").and_then(|v| v.as_str()) == Some("text") {
                    c.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
                } else if let Some(s) = c.as_str() {
                    Some(s.to_string())
                } else {
                    Some(c.to_string())
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn pretty_input(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn tool_summary(arg: &Value) -> String {
    if let Some(fp) = arg.get("file_path").and_then(|v| v.as_str()) {
        if !fp.is_empty() {
            return clip(fp, 46);
        }
    }
    if let Some(p) = arg.get("path").and_then(|v| v.as_str()) {
        if !p.is_empty() {
            return clip(p, 46);
        }
    }
    if let Some(cmd) = arg.get("command") {
        let s = match cmd {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !s.is_empty() && s != "null" {
            return clip(&s, 46);
        }
    }
    if let Some(pat) = arg.get("pattern") {
        let s = match pat {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !s.is_empty() && s != "null" {
            return clip(&s, 46);
        }
    }
    if let Some(d) = arg.get("description").and_then(|v| v.as_str()) {
        if !d.is_empty() {
            return clip(d, 46);
        }
    }
    String::new()
}

/// Best-effort codex: user/assistant text only (no verified tool/thinking shape).
fn parse_codex_best_effort(path: &Path) -> Vec<TranscriptItem> {
    let mut items = Vec::new();
    for line in read_lines(path) {
        let e = match parse_json_object(&line) {
            Some(v) => v,
            None => continue,
        };
        let typ = e.get("type").and_then(|v| v.as_str()).unwrap_or("");
        // Common shapes: response_item / event_msg with payload
        if typ == "response_item" {
            let payload = e.get("payload").cloned().unwrap_or(Value::Null);
            let role = payload
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content = payload.get("content");
            let texts = extract_codex_texts(content);
            for t in texts {
                if is_noise(&t) {
                    continue;
                }
                if role == "user" {
                    items.push(TranscriptItem::User { text: t });
                } else if role == "assistant" {
                    items.push(TranscriptItem::Assistant { text: t });
                }
            }
        }
    }
    items
}

fn extract_codex_texts(content: Option<&Value>) -> Vec<String> {
    match content {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|c| {
                let t = c.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if t == "input_text" || t == "output_text" || t == "text" {
                    c.get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_tool_use_and_result_from_fixture() {
        let dir = std::env::temp_dir().join(format!("hv-tx-{}", short_tmp()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sess.jsonl");
        let lines = [
            r#"{"type":"user","message":{"role":"user","content":"list the files"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":""},{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"cargo test","description":"run tests"}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":true,"content":"Exit code 1\nFAILED"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"tests failed"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"I should retry with --lib"}]}}"#,
        ];
        fs::write(&path, lines.join("\n")).unwrap();
        let items = parse_transcript(path.to_str().unwrap(), "claude code", 0);
        assert!(matches!(&items[0], TranscriptItem::User { text } if text == "list the files"));
        match &items[1] {
            TranscriptItem::Tool {
                name,
                summary,
                result,
                is_error,
                ..
            } => {
                assert_eq!(name, "Bash");
                assert!(summary.contains("cargo test"), "{summary}");
                assert_eq!(result.as_deref(), Some("Exit code 1\nFAILED"));
                assert!(*is_error);
            }
            other => panic!("expected tool, got {other:?}"),
        }
        // empty thinking skipped; nonempty thinking present
        assert!(matches!(&items[2], TranscriptItem::Assistant { .. }));
        assert!(matches!(&items[3], TranscriptItem::Thinking { text } if text.contains("retry")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn real_claude_jsonl_has_tools() {
        let home = std::env::var("HOME").unwrap_or_default();
        let pattern = format!("{home}/.claude/projects/-Users-joe-git-gx/*.jsonl");
        let paths: Vec<_> = match glob::glob(&pattern) {
            Ok(g) => g.filter_map(|r| r.ok()).collect(),
            Err(_) => return, // skip if no local data
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        let items = parse_transcript(path.to_str().unwrap(), "claude code", 200);
        let tools = items
            .iter()
            .filter(|i| matches!(i, TranscriptItem::Tool { .. }))
            .count();
        assert!(tools > 0, "expected tool_use in {}", path.display());
        // empty thinking must not produce placeholders
        let thinking = items
            .iter()
            .filter(|i| matches!(i, TranscriptItem::Thinking { .. }))
            .count();
        assert_eq!(thinking, 0, "local transcripts have empty thinking fields");
    }

    fn short_tmp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
