//! Read-only adapter over `~/.local/share/opencode/opencode.db`.
//! Legacy JSON under `storage/` is ignored — the sqlite db is the live store.

use super::*;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct OpencodeAdapter;

impl Adapter for OpencodeAdapter {
    fn scan(&self, max_age_hours: f64, limit: usize) -> Vec<Session> {
        match scan_raw(max_age_hours, limit) {
            Ok(raw) => finalize(raw),
            Err(e) => {
                eprintln!("[scan_opencode] {e}");
                Vec::new()
            }
        }
    }
}

pub fn db_path() -> PathBuf {
    PathBuf::from(format!("{}/.local/share/opencode/opencode.db", home_dir()))
}

pub fn scan_raw(max_age_hours: f64, limit: usize) -> Result<Vec<RawSession>, String> {
    let db = db_path();
    if !db.exists() {
        return Ok(Vec::new());
    }
    let src = db.to_string_lossy().to_string();
    // DECISION: mode=ro only (no immutable=1) — see open_ro.
    let con = open_ro(&db)?;

    let now = now_secs();
    let limit_i = (limit * 4) as i64;
    let mut stmt = con
        .prepare(
            "SELECT id, parent_id, directory, title, model, time_created, time_updated, time_archived \
             FROM session ORDER BY time_updated DESC LIMIT ?",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        i64,
        i64,
        Option<i64>,
    )> = stmt
        .query_map([limit_i], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut sidechains: HashMap<String, u32> = HashMap::new();
    let mut candidates: Vec<(String, String, String, String, f64)> = Vec::new();

    for (id, parent_id, directory, title, model, _created, updated, archived) in rows {
        if archived.is_some() {
            continue;
        }
        // time_* are epoch milliseconds → Session.mtime is seconds.
        let mtime = (updated as f64) / 1000.0;
        if mtime == 0.0 || now - mtime > max_age_hours * 3600.0 {
            continue;
        }
        if let Some(parent) = parent_id {
            *sidechains.entry(parent).or_insert(0) += 1;
            continue;
        }
        candidates.push((
            id,
            directory,
            title,
            model_display(model.as_deref().unwrap_or("")),
            mtime,
        ));
    }

    let mut out = Vec::new();
    for (id, directory, title, model, mtime) in candidates {
        // Encode the session id into src so the transcript reader knows which
        // session's message/part rows to read back out of the shared db.
        let s_src = format!("{src}#{id}");
        let mut s = empty_raw("opencode", &id, mtime, &s_src);
        s.title = title;
        s.model = model;
        s.cwd = directory;
        s.branch = String::new();
        s.sidechains = sidechains.remove(&id).unwrap_or(0);
        fill_messages(&con, &id, &mut s);
        out.push(s);
    }

    out.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    Ok(out)
}

fn open_ro(db: &PathBuf) -> Result<Connection, String> {
    // DECISION: do NOT use immutable=1. opencode.db is WAL with a live writer
    // (`opencode serve` / TUI); immutable=1 freezes a stale snapshot and drops
    // recent message/part rows while session title/model may still look fresh.
    let uri = format!("file:{}?mode=ro", db.display());
    Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| e.to_string())
}

pub fn model_display(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
    let provider = v.get("providerID").and_then(|x| x.as_str()).unwrap_or("");
    if !provider.is_empty() && !id.is_empty() {
        format!("{provider}/{id}")
    } else if !id.is_empty() {
        id.to_string()
    } else {
        String::new()
    }
}

pub fn tool_activity(data: &Value) -> Option<String> {
    if data.get("type").and_then(|t| t.as_str()) != Some("tool") {
        return None;
    }
    let name = data.get("tool").and_then(|t| t.as_str()).unwrap_or("tool");
    let input = data
        .pointer("/state/input")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    let hint = tool_hint(&input);
    Some(format!("⚒ {name}({})", clip(&hint, 46)))
}

fn tool_hint(arg: &Value) -> String {
    for key in ["filePath", "file_path", "path", "command", "pattern", "description"] {
        if let Some(v) = arg.get(key) {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if !s.is_empty() && s != "null" {
                return s;
            }
        }
    }
    // first string field in the object
    if let Value::Object(map) = arg {
        for (_k, v) in map {
            if let Value::String(s) = v {
                if !s.is_empty() {
                    return s.clone();
                }
            }
        }
    }
    String::new()
}

fn fill_messages(con: &Connection, sid: &str, s: &mut RawSession) {
    let mut stmt = match con.prepare(
        "SELECT m.data, p.data FROM part p \
         JOIN message m ON m.id = p.message_id \
         WHERE p.session_id = ? \
         ORDER BY p.time_created ASC, p.id ASC",
    ) {
        Ok(st) => st,
        Err(_) => return,
    };
    let rows = match stmt.query_map([sid], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(mapped) => mapped.filter_map(|r| r.ok()).collect::<Vec<_>>(),
        Err(_) => return,
    };

    for (msg_data, part_data) in rows {
        let msg: Value = serde_json::from_str(&msg_data).unwrap_or(Value::Null);
        let part: Value = serde_json::from_str(&part_data).unwrap_or(Value::Null);
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let ptype = part.get("type").and_then(|t| t.as_str()).unwrap_or("");

        if ptype == "text" {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() && !is_noise(text) {
                    if role == "user" {
                        s.last_user = text.to_string();
                        s.last_role = "user".into();
                    } else if role == "assistant" {
                        s.last_assistant = text.to_string();
                        s.last_role = "assistant".into();
                    }
                }
            }
        } else if ptype == "tool" {
            if let Some(act) = tool_activity(&part) {
                s.activity = act;
                s.last_role = "assistant".into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fixture_db() -> (Tmp, PathBuf) {
        // DECISION: std::env::temp_dir — tempfile crate not in deps.
        let dir = std::env::temp_dir().join(format!(
            "hv-opencode-test-{}-{}",
            std::process::id(),
            now_secs() as u64
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opencode.db");
        let con = Connection::open(&path).unwrap();
        con.execute_batch(
            "CREATE TABLE session (
                id text PRIMARY KEY,
                project_id text NOT NULL,
                parent_id text,
                slug text NOT NULL,
                directory text NOT NULL,
                title text NOT NULL,
                version text NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL,
                time_archived integer,
                model text
            );
            CREATE TABLE message (
                id text PRIMARY KEY,
                session_id text NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL,
                data text NOT NULL
            );
            CREATE TABLE part (
                id text PRIMARY KEY,
                message_id text NOT NULL,
                session_id text NOT NULL,
                time_created integer NOT NULL,
                time_updated integer NOT NULL,
                data text NOT NULL
            );",
        )
        .unwrap();

        let now_ms = (now_secs() * 1000.0) as i64;
        con.execute(
            "INSERT INTO session VALUES ('ses_top','proj',NULL,'slug','/tmp/repo','Hello title','1',?1,?1,NULL,?2)",
            rusqlite::params![
                now_ms,
                r#"{"id":"gpt-5","providerID":"openai","variant":"high"}"#
            ],
        )
        .unwrap();
        con.execute(
            "INSERT INTO session VALUES ('ses_arch','proj',NULL,'slug','/tmp/repo','Archived','1',?1,?1,?1,NULL)",
            [now_ms],
        )
        .unwrap();
        con.execute(
            "INSERT INTO session VALUES ('ses_child','proj','ses_top','slug','/tmp/repo','Child','1',?1,?1,NULL,NULL)",
            [now_ms],
        )
        .unwrap();
        con.execute(
            "INSERT INTO session VALUES ('ses_old','proj',NULL,'slug','/tmp/old','Old','1',1,1,NULL,NULL)",
            [],
        )
        .unwrap();

        con.execute(
            "INSERT INTO message VALUES ('msg_u','ses_top',?1,?1,?2)",
            rusqlite::params![now_ms, r#"{"role":"user","time":{"created":1}}"#],
        )
        .unwrap();
        con.execute(
            "INSERT INTO part VALUES ('prt_u','msg_u','ses_top',?1,?1,?2)",
            rusqlite::params![now_ms, r#"{"type":"text","text":"hello user"}"#],
        )
        .unwrap();
        con.execute(
            "INSERT INTO message VALUES ('msg_a','ses_top',?1,?1,?2)",
            rusqlite::params![
                now_ms + 1,
                r#"{"role":"assistant","time":{"created":2}}"#
            ],
        )
        .unwrap();
        con.execute(
            "INSERT INTO part VALUES ('prt_t','msg_a','ses_top',?1,?1,?2)",
            rusqlite::params![
                now_ms + 1,
                r#"{"type":"tool","tool":"edit","state":{"status":"completed","input":{"filePath":"/tmp/repo/src/main.rs"}}}"#
            ],
        )
        .unwrap();
        con.execute(
            "INSERT INTO part VALUES ('prt_a','msg_a','ses_top',?1,?1,?2)",
            rusqlite::params![
                now_ms + 2,
                r#"{"type":"text","text":"done editing"}"#
            ],
        )
        .unwrap();

        (Tmp(dir), path)
    }

    #[test]
    fn model_json_to_display() {
        assert_eq!(
            model_display(r#"{"id":"gpt-5","providerID":"openai","variant":"high"}"#),
            "openai/gpt-5"
        );
        assert_eq!(model_display(""), "");
        assert_eq!(model_display("not-json"), "not-json");
    }

    #[test]
    fn tool_part_to_activity() {
        let v: Value = serde_json::from_str(
            r#"{"type":"tool","tool":"edit","state":{"status":"completed","input":{"filePath":"/tmp/repo/src/main.rs"}}}"#,
        )
        .unwrap();
        let act = tool_activity(&v).unwrap();
        assert!(act.contains("edit"), "{act}");
        assert!(act.contains("main.rs"), "{act}");
    }

    #[test]
    fn ms_to_s_and_filters() {
        let (_guard, path) = fixture_db();
        let con = open_ro(&path).unwrap();
        let now = now_secs();
        let mut stmt = con
            .prepare(
                "SELECT id, parent_id, directory, title, model, time_created, time_updated, time_archived \
                 FROM session ORDER BY time_updated DESC",
            )
            .unwrap();
        let rows: Vec<(
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            i64,
            i64,
            Option<i64>,
        )> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let mut sidechains: HashMap<String, u32> = HashMap::new();
        let mut kept = Vec::new();
        for (id, parent_id, directory, title, model, _c, updated, archived) in &rows {
            if archived.is_some() {
                continue;
            }
            let mtime = (*updated as f64) / 1000.0;
            assert!(
                (mtime - now).abs() < 5.0 || *updated == 1,
                "mtime={mtime} now={now}"
            );
            if now - mtime > 48.0 * 3600.0 {
                continue;
            }
            if let Some(p) = parent_id {
                *sidechains.entry(p.clone()).or_insert(0) += 1;
                continue;
            }
            kept.push((
                id.clone(),
                directory.clone(),
                title.clone(),
                model_display(model.as_deref().unwrap_or("")),
                mtime,
            ));
        }

        assert_eq!(kept.len(), 1, "only top-level non-archived in window");
        assert_eq!(kept[0].0, "ses_top");
        assert_eq!(kept[0].3, "openai/gpt-5");
        assert_eq!(sidechains.get("ses_top"), Some(&1));

        let mut s = empty_raw("opencode", "ses_top", kept[0].4, "");
        fill_messages(&con, "ses_top", &mut s);
        assert_eq!(s.last_user, "hello user");
        assert_eq!(s.last_assistant, "done editing");
        assert!(s.activity.contains("edit"), "{}", s.activity);
        assert_eq!(s.last_role, "assistant");
    }
}
