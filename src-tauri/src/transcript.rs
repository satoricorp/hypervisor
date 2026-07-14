//! On-demand transcript parse for the selected session.
//! NOT used by the snapshot hot loop — keep registry::watch_sessions untouched.

use crate::adapters::{clip, is_noise, parse_json_object, read_lines};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Cap a tool result / output body so a single file-read can't bloat the pane.
/// Preserves newlines (unlike `clip`, which collapses whitespace).
fn clip_result(s: &str) -> String {
    const MAX: usize = 4000;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        let head: String = s.chars().take(MAX).collect();
        format!("{head}\n… (truncated)")
    }
}

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
    let limit = if limit == 0 { 400 } else { limit };
    // cursor / opencode keep conversations in SQLite, not JSONL. Their adapters
    // encode `src` as "<db-path>#<session-id>" so we know which rows to read.
    let items = match harness {
        "cursor" => parse_cursor(src),
        "opencode" => parse_opencode(src),
        _ => {
            let path = Path::new(src);
            if !path.exists() {
                return Vec::new();
            }
            if harness == "claude code" || src.contains("/.claude/projects/") {
                parse_claude(path)
            } else if harness == "codex" || src.contains("/.codex/") {
                parse_codex_best_effort(path)
            } else {
                // unknown harness — no verified shape
                Vec::new()
            }
        }
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

/// Cursor stores each conversation message as a `bubbleId:<composerId>:<bubbleId>`
/// row in the `cursorDiskKV` table of `state.vscdb`; the ordered list of bubble
/// refs lives under `composerData:<composerId>`. The cursor adapter encodes
/// `src` as "<db-path>#<composerId>" so we can find both.
fn parse_cursor(src: &str) -> Vec<TranscriptItem> {
    let (db, cid) = match src.rsplit_once('#') {
        Some((d, c)) if !c.is_empty() => (d, c),
        _ => return Vec::new(),
    };
    if !Path::new(db).exists() {
        return Vec::new();
    }
    // immutable=1 is safe here: Cursor's KV store is not a live-WAL hot writer
    // the way opencode.db is, and the adapter already reads it this way.
    let uri = format!("file:{db}?mode=ro&immutable=1");
    let con = match Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    ) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let data: String = match con.query_row(
        "SELECT value FROM cursorDiskKV WHERE key = ?1",
        [format!("composerData:{cid}")],
        |r| r.get(0),
    ) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let cdata: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
    let headers = match cdata
        .get("fullConversationHeadersOnly")
        .and_then(|v| v.as_array())
    {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut stmt = match con.prepare("SELECT value FROM cursorDiskKV WHERE key = ?1") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut items = Vec::new();
    for h in headers {
        let bid = h.get("bubbleId").and_then(|v| v.as_str()).unwrap_or("");
        if bid.is_empty() {
            continue;
        }
        let raw: String = match stmt.query_row([format!("bubbleId:{cid}:{bid}")], |r| r.get(0)) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Ok(b) = serde_json::from_str::<Value>(&raw) {
            push_cursor_bubble(&mut items, &b);
        }
    }
    items
}

/// Map one Cursor bubble to transcript item(s): thinking, then a tool call,
/// then the message text (type 1 = user, 2 = assistant).
fn push_cursor_bubble(items: &mut Vec<TranscriptItem>, b: &Value) {
    let typ = b.get("type").and_then(|v| v.as_i64()).unwrap_or(0);
    if let Some(t) = b
        .get("thinking")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
    {
        let t = t.trim();
        if !t.is_empty() {
            items.push(TranscriptItem::Thinking { text: t.to_string() });
        }
    }
    if let Some(tfd) = b.get("toolFormerData").filter(|v| v.is_object()) {
        let name = tfd
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("tool")
            .to_string();
        let args_raw = tfd.get("rawArgs").and_then(|v| v.as_str()).unwrap_or("");
        let args_val: Value = serde_json::from_str(args_raw).unwrap_or(Value::Null);
        let summary = tool_summary(&args_val);
        let input = if args_val.is_null() {
            args_raw.to_string()
        } else {
            pretty_input(&args_val)
        };
        let status = tfd.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let result = tfd
            .get("result")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(clip_result);
        items.push(TranscriptItem::Tool {
            id: tfd
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name,
            summary,
            input,
            result,
            is_error: status == "error",
        });
    }
    if let Some(text) = b.get("text").and_then(|v| v.as_str()) {
        let text = text.trim();
        if !text.is_empty() && !is_noise(text) {
            if typ == 1 {
                items.push(TranscriptItem::User { text: text.to_string() });
            } else if typ == 2 {
                items.push(TranscriptItem::Assistant { text: text.to_string() });
            }
        }
    }
}

/// opencode keeps messages/parts in `opencode.db`. The adapter encodes `src` as
/// "<db-path>#<sessionId>". Mirrors the adapter's `fill_messages` query but
/// emits full transcript items (text, reasoning→thinking, tool with output).
fn parse_opencode(src: &str) -> Vec<TranscriptItem> {
    let (db, sid) = match src.rsplit_once('#') {
        Some((d, s)) if !s.is_empty() => (d, s),
        _ => return Vec::new(),
    };
    if !Path::new(db).exists() {
        return Vec::new();
    }
    // DECISION: mode=ro only (no immutable=1) — opencode.db is a live-WAL store;
    // immutable would freeze a stale snapshot and drop recent parts.
    let uri = format!("file:{db}?mode=ro");
    let con = match Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    ) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut stmt = match con.prepare(
        "SELECT m.data, p.data FROM part p \
         JOIN message m ON m.id = p.message_id \
         WHERE p.session_id = ?1 \
         ORDER BY p.time_created ASC, p.id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map([sid], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(m) => m.filter_map(|r| r.ok()).collect::<Vec<_>>(),
        Err(_) => return Vec::new(),
    };
    let mut items = Vec::new();
    for (msg_data, part_data) in rows {
        let msg: Value = serde_json::from_str(&msg_data).unwrap_or(Value::Null);
        let part: Value = serde_json::from_str(&part_data).unwrap_or(Value::Null);
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        match part.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "text" => {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    let text = text.trim();
                    if !text.is_empty() && !is_noise(text) {
                        if role == "user" {
                            items.push(TranscriptItem::User { text: text.to_string() });
                        } else if role == "assistant" {
                            items.push(TranscriptItem::Assistant { text: text.to_string() });
                        }
                    }
                }
            }
            "reasoning" => {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    let text = text.trim();
                    if !text.is_empty() {
                        items.push(TranscriptItem::Thinking { text: text.to_string() });
                    }
                }
            }
            "tool" => {
                let name = part
                    .get("tool")
                    .and_then(|t| t.as_str())
                    .unwrap_or("tool")
                    .to_string();
                let state = part.get("state").cloned().unwrap_or(Value::Null);
                let input_val = state.get("input").cloned().unwrap_or(Value::Null);
                let summary = tool_summary(&input_val);
                let input = if input_val.is_null() {
                    String::new()
                } else {
                    pretty_input(&input_val)
                };
                let status = state.get("status").and_then(|v| v.as_str()).unwrap_or("");
                let result = match state.get("output") {
                    Some(Value::String(s)) if !s.is_empty() => Some(clip_result(s)),
                    Some(Value::Null) | None => None,
                    Some(other) => Some(clip_result(&other.to_string())),
                };
                items.push(TranscriptItem::Tool {
                    id: part
                        .get("callID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name,
                    summary,
                    input,
                    result,
                    is_error: status == "error",
                });
            }
            _ => {}
        }
    }
    items
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

    #[test]
    fn cursor_bubbles_parse_in_order() {
        let dir = std::env::temp_dir().join(format!("hv-cur-{}", short_tmp()));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("state.vscdb");
        let con = Connection::open(&db).unwrap();
        con.execute_batch("CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT);")
            .unwrap();
        let cid = "comp-1";
        let cdata = r#"{"fullConversationHeadersOnly":[
            {"bubbleId":"b1","type":1},
            {"bubbleId":"b2","type":2},
            {"bubbleId":"b3","type":2}
        ]}"#;
        let put = |k: String, v: &str| {
            con.execute("INSERT INTO cursorDiskKV VALUES (?1, ?2)", rusqlite::params![k, v])
                .unwrap();
        };
        put(format!("composerData:{cid}"), cdata);
        put(format!("bubbleId:{cid}:b1"), r#"{"type":1,"text":"do the thing"}"#);
        put(
            format!("bubbleId:{cid}:b2"),
            r#"{"type":2,"thinking":{"text":"planning"},"toolFormerData":{"name":"read_file_v2","rawArgs":"{\"path\":\"/tmp/x.rs\"}","result":"{\"contents\":\"hi\"}","status":"completed"}}"#,
        );
        put(format!("bubbleId:{cid}:b3"), r#"{"type":2,"text":"all done"}"#);
        drop(con);
        let src = format!("{}#{}", db.to_str().unwrap(), cid);
        let items = parse_transcript(&src, "cursor", 0);
        assert!(matches!(&items[0], TranscriptItem::User { text } if text == "do the thing"));
        assert!(matches!(&items[1], TranscriptItem::Thinking { text } if text == "planning"));
        match &items[2] {
            TranscriptItem::Tool { name, summary, result, is_error, .. } => {
                assert_eq!(name, "read_file_v2");
                assert!(summary.contains("x.rs"), "{summary}");
                assert!(result.as_deref().unwrap().contains("contents"));
                assert!(!is_error);
            }
            other => panic!("expected tool, got {other:?}"),
        }
        assert!(matches!(&items[3], TranscriptItem::Assistant { text } if text == "all done"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn opencode_parts_parse_in_order() {
        let dir = std::env::temp_dir().join(format!("hv-oc-tx-{}", short_tmp()));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("opencode.db");
        let con = Connection::open(&db).unwrap();
        con.execute_batch(
            "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT);
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
        )
        .unwrap();
        con.execute("INSERT INTO message VALUES ('m1', ?1)", [r#"{"role":"user"}"#]).unwrap();
        con.execute("INSERT INTO message VALUES ('m2', ?1)", [r#"{"role":"assistant"}"#]).unwrap();
        let part = |id: &str, mid: &str, t: i64, data: &str| {
            con.execute(
                "INSERT INTO part VALUES (?1, ?2, 's1', ?3, ?4)",
                rusqlite::params![id, mid, t, data],
            )
            .unwrap();
        };
        part("p1", "m1", 1, r#"{"type":"text","text":"fix the bug"}"#);
        part("p2", "m2", 2, r#"{"type":"reasoning","text":"looking at it"}"#);
        part(
            "p3",
            "m2",
            3,
            r#"{"type":"tool","tool":"grep","callID":"c1","state":{"status":"completed","input":{"pattern":"foo"},"output":"3 matches"}}"#,
        );
        part("p4", "m2", 4, r#"{"type":"text","text":"found it"}"#);
        drop(con);
        let src = format!("{}#{}", db.to_str().unwrap(), "s1");
        let items = parse_transcript(&src, "opencode", 0);
        assert!(matches!(&items[0], TranscriptItem::User { text } if text == "fix the bug"));
        assert!(matches!(&items[1], TranscriptItem::Thinking { text } if text == "looking at it"));
        match &items[2] {
            TranscriptItem::Tool { name, summary, result, .. } => {
                assert_eq!(name, "grep");
                assert!(summary.contains("foo"), "{summary}");
                assert_eq!(result.as_deref(), Some("3 matches"));
            }
            other => panic!("expected tool, got {other:?}"),
        }
        assert!(matches!(&items[3], TranscriptItem::Assistant { text } if text == "found it"));
        let _ = fs::remove_dir_all(&dir);
    }

    // Real-data smoke tests: confirm the SQLite readers match this machine's
    // actual Cursor / opencode schemas. Skip cleanly when no local data exists.
    #[test]
    fn real_cursor_db_parses_when_present() {
        let home = std::env::var("HOME").unwrap_or_default();
        let db =
            format!("{home}/Library/Application Support/Cursor/User/globalStorage/state.vscdb");
        if !Path::new(&db).exists() {
            return;
        }
        let con = match Connection::open_with_flags(
            &format!("file:{db}?mode=ro&immutable=1"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return,
        };
        // The composer with the most bubble rows definitely has a transcript.
        let cid: String = match con.query_row(
            "SELECT substr(key, 10, 36) FROM cursorDiskKV WHERE key LIKE 'bubbleId:%' \
             GROUP BY substr(key, 10, 36) ORDER BY count(*) DESC LIMIT 1",
            [],
            |r| r.get(0),
        ) {
            Ok(v) => v,
            Err(_) => return, // no bubbles on this machine
        };
        let items = parse_transcript(&format!("{db}#{cid}"), "cursor", 400);
        assert!(!items.is_empty(), "cursor composer {cid} produced no items");
    }

    #[test]
    fn real_opencode_db_parses_when_present() {
        let home = std::env::var("HOME").unwrap_or_default();
        let db = format!("{home}/.local/share/opencode/opencode.db");
        if !Path::new(&db).exists() {
            return;
        }
        let con = match Connection::open_with_flags(
            &format!("file:{db}?mode=ro"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return,
        };
        // A session that has a text part is guaranteed to yield a message.
        let sid: String = match con.query_row(
            "SELECT session_id FROM part WHERE json_extract(data,'$.type')='text' \
             ORDER BY time_created DESC LIMIT 1",
            [],
            |r| r.get(0),
        ) {
            Ok(v) => v,
            Err(_) => return, // no text parts on this machine
        };
        let items = parse_transcript(&format!("{db}#{sid}"), "opencode", 400);
        assert!(!items.is_empty(), "opencode session {sid} produced no items");
    }

    fn short_tmp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
