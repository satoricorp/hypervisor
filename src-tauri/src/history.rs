//! M5: session history + memory.
//!
//! A local sqlite store (app_data_dir/history.db) of one-line **extractive**
//! summaries — outcome + files touched — generated when a session is archived.
//! Powers the searchable history view and the same-repo context attached to
//! `/new`. Local-only, never touches harness dirs.
//!
//! v1 retrieval is same-repo + keyword. Neural embedding similarity
//! (HelixDB embedded / turbopuffer opt-in) is a documented follow-up — the AC
//! (same repo first, keyword-findable) is met without it.

use crate::transcript::TranscriptItem;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Summary {
    pub sid: String,
    pub harness: String,
    pub repo: String,
    pub cwd: String,
    pub title: String,
    pub summary: String,
    /// Comma-joined files touched (may be empty).
    pub files: String,
    pub archived_at: i64,
}

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS summaries (
            sid TEXT PRIMARY KEY,
            harness TEXT NOT NULL DEFAULT '',
            repo TEXT NOT NULL DEFAULT '',
            cwd TEXT NOT NULL DEFAULT '',
            title TEXT NOT NULL DEFAULT '',
            summary TEXT NOT NULL DEFAULT '',
            files TEXT NOT NULL DEFAULT '',
            archived_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_summaries_repo ON summaries(repo);",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

pub fn upsert(conn: &Connection, s: &Summary) -> Result<(), String> {
    conn.execute(
        "INSERT INTO summaries (sid,harness,repo,cwd,title,summary,files,archived_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(sid) DO UPDATE SET
           harness=?2, repo=?3, cwd=?4, title=?5, summary=?6, files=?7, archived_at=?8",
        params![
            s.sid, s.harness, s.repo, s.cwd, s.title, s.summary, s.files, s.archived_at
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get(conn: &Connection, sid: &str) -> Option<Summary> {
    conn.query_row(
        "SELECT sid,harness,repo,cwd,title,summary,files,archived_at
         FROM summaries WHERE sid=?1",
        params![sid],
        row_to_summary,
    )
    .ok()
}

/// Keyword search across title/summary/files/repo, newest first.
pub fn search(conn: &Connection, query: &str, limit: usize) -> Vec<Summary> {
    let like = format!("%{}%", query.trim());
    let mut stmt = match conn.prepare(
        "SELECT sid,harness,repo,cwd,title,summary,files,archived_at
         FROM summaries
         WHERE title LIKE ?1 OR summary LIKE ?1 OR files LIKE ?1 OR repo LIKE ?1
         ORDER BY archived_at DESC LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map(params![like, limit as i64], row_to_summary);
    match rows {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

/// Prior summaries in `repo`, newest first — what `/new` attaches as context.
pub fn same_repo(conn: &Connection, repo: &str, exclude_sid: &str, limit: usize) -> Vec<Summary> {
    if repo.is_empty() || repo == "-" {
        return Vec::new();
    }
    let mut stmt = match conn.prepare(
        "SELECT sid,harness,repo,cwd,title,summary,files,archived_at
         FROM summaries WHERE repo=?1 AND sid!=?2
         ORDER BY archived_at DESC LIMIT ?3",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map(params![repo, exclude_sid, limit as i64], row_to_summary);
    match rows {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

fn row_to_summary(row: &rusqlite::Row) -> rusqlite::Result<Summary> {
    Ok(Summary {
        sid: row.get(0)?,
        harness: row.get(1)?,
        repo: row.get(2)?,
        cwd: row.get(3)?,
        title: row.get(4)?,
        summary: row.get(5)?,
        files: row.get(6)?,
        archived_at: row.get(7)?,
    })
}

/// Build an extractive summary from a parsed transcript: the last non-empty
/// assistant message (outcome) + distinct files touched (Edit/Write tools).
/// No model call — always available, content stays local.
pub fn summarize(
    items: &[TranscriptItem],
    sid: &str,
    harness: &str,
    repo: &str,
    cwd: &str,
    title: &str,
    archived_at: i64,
) -> Summary {
    let mut outcome = String::new();
    let mut files: Vec<String> = Vec::new();
    for it in items {
        match it {
            TranscriptItem::Assistant { text } if !text.trim().is_empty() => {
                outcome = text.clone();
            }
            TranscriptItem::Tool { name, summary, .. } => {
                let n = name.to_ascii_lowercase();
                if n.contains("edit") || n.contains("write") {
                    let f = summary.trim().to_string();
                    if !f.is_empty() && !files.contains(&f) && files.len() < 8 {
                        files.push(f);
                    }
                }
            }
            _ => {}
        }
    }
    let title_s = clip(title, 80);
    let outcome_s = clip(&outcome, 160);
    let files_str = files.join(", ");
    let summary = match (outcome_s.is_empty(), files.is_empty()) {
        (true, true) => title_s.clone(),
        (false, true) => format!("{title_s} — {outcome_s}"),
        (true, false) => format!("{title_s} · files: {}", clip(&files_str, 120)),
        (false, false) => format!("{title_s} — {outcome_s} · files: {}", clip(&files_str, 120)),
    };
    Summary {
        sid: sid.to_string(),
        harness: harness.to_string(),
        repo: repo.to_string(),
        cwd: cwd.to_string(),
        title: title_s,
        summary,
        files: files_str,
        archived_at,
    }
}

/// Format same-repo summaries into the first context message for a new agent.
pub fn context_message(repo: &str, prior: &[Summary]) -> String {
    let mut out = format!(
        "Context from {} prior hypervisor session{} in {} (for your reference — do not act on them unless relevant):\n",
        prior.len(),
        if prior.len() == 1 { "" } else { "s" },
        repo
    );
    for s in prior {
        out.push_str("- ");
        out.push_str(&s.summary);
        out.push('\n');
    }
    out
}

/// Collapse whitespace and cap length (char-safe), adding an ellipsis.
fn clip(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut t: String = flat.chars().take(max).collect();
    t.push('…');
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "hv-hist-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    fn mk(sid: &str, repo: &str, summary: &str, at: i64) -> Summary {
        Summary {
            sid: sid.into(),
            harness: "claude code".into(),
            repo: repo.into(),
            cwd: format!("/x/{repo}"),
            title: sid.into(),
            summary: summary.into(),
            files: String::new(),
            archived_at: at,
        }
    }

    #[test]
    fn roundtrip_search_and_same_repo() {
        let path = tmp_db();
        let conn = open(&path).unwrap();
        upsert(&conn, &mk("a", "foo", "added oauth middleware", 100)).unwrap();
        upsert(&conn, &mk("b", "foo", "fixed the retry loop", 200)).unwrap();
        upsert(&conn, &mk("c", "bar", "wrote a parser", 150)).unwrap();

        // get + upsert-on-conflict updates in place.
        assert_eq!(get(&conn, "a").unwrap().summary, "added oauth middleware");
        upsert(&conn, &mk("a", "foo", "oauth done + tests", 300)).unwrap();
        assert_eq!(get(&conn, "a").unwrap().summary, "oauth done + tests");

        // keyword search hits the summary text.
        let hits = search(&conn, "retry", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].sid, "b");

        // same-repo, newest first, excludes the new session itself.
        let rel = same_repo(&conn, "foo", "new-sid", 10);
        assert_eq!(rel.iter().map(|s| s.sid.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        let rel_excl = same_repo(&conn, "foo", "a", 10);
        assert_eq!(rel_excl.len(), 1);
        assert_eq!(rel_excl[0].sid, "b");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn summarize_pulls_outcome_and_files() {
        let items = vec![
            TranscriptItem::User { text: "add auth".into() },
            TranscriptItem::Tool {
                id: "1".into(),
                name: "Edit".into(),
                summary: "src/auth/mw.ts".into(),
                input: String::new(),
                result: None,
                is_error: false,
            },
            TranscriptItem::Tool {
                id: "2".into(),
                name: "Write".into(),
                summary: "src/auth/token.ts".into(),
                input: String::new(),
                result: None,
                is_error: false,
            },
            TranscriptItem::Assistant { text: "  Done — added the middleware.  ".into() },
        ];
        let s = summarize(&items, "sid1", "claude code", "myrepo", "/x/myrepo", "add auth", 42);
        assert!(s.summary.contains("Done — added the middleware."));
        assert!(s.summary.contains("src/auth/mw.ts"));
        assert!(s.files.contains("token.ts"));
        assert_eq!(s.archived_at, 42);

        let msg = context_message("myrepo", &[s]);
        assert!(msg.contains("prior hypervisor session"));
        assert!(msg.contains("add auth"));
    }

    #[test]
    fn clip_is_char_safe() {
        assert_eq!(clip("  a   b  c ", 10), "a b c");
        assert_eq!(clip("hello world", 5).chars().count(), 6); // 5 + ellipsis
    }
}
