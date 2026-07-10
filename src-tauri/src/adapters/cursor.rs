use super::*;
use glob::glob;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct CursorAdapter;

impl Adapter for CursorAdapter {
    fn scan(&self, max_age_hours: f64, limit: usize) -> Vec<Session> {
        match scan_raw(max_age_hours, limit) {
            Ok(raw) => finalize(raw),
            Err(e) => {
                eprintln!("[scan_cursor] {e}");
                Vec::new()
            }
        }
    }
}

pub fn scan_raw(max_age_hours: f64, limit: usize) -> Result<Vec<RawSession>, String> {
    let home = home_dir();
    let base = format!("{home}/Library/Application Support/Cursor/User");
    let db = format!("{base}/globalStorage/state.vscdb");
    if !PathBuf::from(&db).exists() {
        return Ok(Vec::new());
    }

    let mut folders: HashMap<String, String> = HashMap::new();
    let wj_pattern = format!("{base}/workspaceStorage/*/workspace.json");
    if let Ok(paths) = glob(&wj_pattern) {
        for path in paths.filter_map(|r| r.ok()) {
            let wsid = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            match std::fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<Value>(&text) {
                    Ok(v) => {
                        let folder = v
                            .get("folder")
                            .and_then(|f| f.as_str())
                            .unwrap_or("")
                            .replace("file://", "");
                        folders.insert(wsid, folder);
                    }
                    Err(_) => {}
                },
                Err(_) => {}
            }
        }
    }

    let now = now_secs();
    let uri = format!("file:{db}?mode=ro&immutable=1");
    let con = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| e.to_string())?;

    let mut stmt = con
        .prepare(
            "SELECT composerId, workspaceId, lastUpdatedAt, isSubagent, value \
             FROM composerHeaders WHERE isArchived=0 \
             ORDER BY lastUpdatedAt DESC LIMIT ?",
        )
        .map_err(|e| e.to_string())?;
    let limit_i = (limit * 4) as i64;
    let rows: Vec<(Option<String>, Option<String>, Option<i64>, Option<i64>, Option<String>)> =
        stmt
            .query_map([limit_i], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

    let mut out: Vec<(RawSession, Option<String>)> = Vec::new();
    let mut subcount: HashMap<String, u32> = HashMap::new();

    for (cid, wsid, upd, is_sub, value) in rows {
        let ts = (upd.unwrap_or(0) as f64) / 1000.0;
        if ts == 0.0 || now - ts > max_age_hours * 3600.0 {
            continue;
        }
        let wsid = wsid.unwrap_or_default();
        if is_sub.unwrap_or(0) != 0 {
            *subcount.entry(wsid).or_insert(0) += 1;
            continue;
        }
        let v: Value = match value.as_deref() {
            Some(s) if !s.is_empty() => serde_json::from_str(s).unwrap_or(Value::Object(Default::default())),
            _ => Value::Object(Default::default()),
        };
        let cid = cid.unwrap_or_default();
        let sid = if cid.len() >= 8 {
            cid[..8].to_string()
        } else {
            cid
        };
        let title = v
            .get("name")
            .and_then(|n| n.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("untitled composer")
            .to_string();
        let model = v
            .get("modelName")
            .and_then(|m| m.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| v.get("unifiedMode").and_then(|m| m.as_str()))
            .unwrap_or("")
            .to_string();
        let last_assistant = if v
            .get("hasUnreadMessages")
            .and_then(|h| h.as_bool())
            .unwrap_or(false)
        {
            "unread response".to_string()
        } else {
            String::new()
        };
        let cwd = folders.get(&wsid).cloned().unwrap_or_default();
        let mut s = empty_raw("cursor", &sid, ts, &db);
        s.title = title;
        s.model = model;
        s.cwd = cwd;
        s.last_assistant = last_assistant;
        s.last_role = "assistant".into();
        out.push((s, Some(wsid)));
    }

    // attach recent subagent count to that workspace's newest session
    for (s, wsid) in &mut out {
        if let Some(ws) = wsid.take() {
            s.sidechains = subcount.remove(&ws).unwrap_or(0);
        }
    }

    let mut sessions: Vec<RawSession> = out.into_iter().map(|(s, _)| s).collect();
    sessions.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    sessions.truncate(limit);
    Ok(sessions)
}
