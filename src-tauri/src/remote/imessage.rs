//! M8b — iMessage bridge: poll self-chat, run shared grammar executor, reply.
//!
//! // DECISION: open chat.db with `mode=ro` only (no `immutable=1`). M2c's
//! opencode.db lesson: `immutable=1` freezes a stale WAL snapshot and drops
//! newest rows — here that would hide the command you just texted.
//!
//! // DECISION: approve/deny use the shared M7g grammar (bare letter / `N: text`),
//! not the literal `approve 5` / `deny 5` in design/remote.md §M8b (pre-M7g).
//! One language, two transports.
//!
//! // DECISION: after each outbound reply, re-read MAX(message.ROWID) and advance
//! the watermark past our own send so the self-reply loop cannot fire.
//!
//! // DECISION: self-chat = 1:1 chat whose `chat_identifier` equals its sole
//! `handle.id` (you texted your own address). Own-handle allowlist = those ids.
//! Commands require `is_from_me = 1` in such a chat. Non-self chats / inbound
//! `is_from_me = 0` are ignored.

use crate::events::{self, AppState, SessionsUpdate};
use crate::grammar::Action;
use crate::remote;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::AppHandle;

const POLL_SECS: u64 = 2;
const PUSH_MIN_GAP: Duration = Duration::from_secs(30);
const LOGIN: &str = "imessage";

pub const APPROVALS_DISABLED_REPLY: &str =
    "approvals are disabled over imessage — use the tailnet page";

/// Refusal when Approve/Deny is planned but `imessage_approvals` is off.
pub fn gate_action(action: Action, approvals_enabled: bool) -> Result<Action, String> {
    match &action {
        Action::Approve { .. } | Action::Deny { .. } if !approvals_enabled => {
            Err(APPROVALS_DISABLED_REPLY.into())
        }
        _ => Ok(action),
    }
}

/// Mac absolute time → unix seconds.
/// Current macOS stores nanoseconds since 2001-01-01 UTC; ancient builds used seconds.
pub fn mac_absolute_to_unix(date: i64) -> i64 {
    const MAC_EPOCH: i64 = 978_307_200; // 2001-01-01 UTC in unix seconds
    // |date| > ~3e10 ⇒ nanoseconds (year ~2001 in ns is ~0; 2020s ≈ 7e17).
    // Seconds since 2001 for 2020s ≈ 7e8 — well below 1e12.
    if date.abs() > 1_000_000_000_000 {
        MAC_EPOCH + date / 1_000_000_000
    } else {
        MAC_EPOCH + date
    }
}

/// Extract plain text from an NSAttributedString / streamtyped `attributedBody` blob.
pub fn decode_attributed_body(blob: &[u8]) -> Option<String> {
    if blob.is_empty() {
        return None;
    }
    if let Some(s) = extract_nsstring(blob) {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

fn extract_nsstring(data: &[u8]) -> Option<String> {
    // streamtyped embeds the body after an "NSString" / "NSMutableString" class name.
    for marker in [b"NSString".as_slice(), b"NSMutableString".as_slice()] {
        let mut search = 0usize;
        while let Some(rel) = find_subslice(&data[search..], marker) {
            let after = search + rel + marker.len();
            // Skip a short window of type/version bytes, then try length-prefixed UTF-8.
            for j in after..data.len().min(after + 64) {
                if let Some(s) = try_len_prefixed_utf8(&data[j..]) {
                    if looks_like_message_body(&s) {
                        return Some(s);
                    }
                }
            }
            search = after;
        }
    }
    None
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len())
        .position(|w| w == needle)
}

fn try_len_prefixed_utf8(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    // Common: single-byte length then UTF-8, or 0x81 / 0x82 + u16/u32 length.
    let (len, start) = if data[0] < 0x80 {
        (data[0] as usize, 1usize)
    } else if data[0] == 0x81 && data.len() >= 3 {
        (u16::from_le_bytes([data[1], data[2]]) as usize, 3usize)
    } else if data[0] == 0x82 && data.len() >= 5 {
        (
            u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize,
            5usize,
        )
    } else {
        return None;
    };
    if len == 0 || len > 8_000 || start + len > data.len() {
        return None;
    }
    let slice = &data[start..start + len];
    String::from_utf8(slice.to_vec()).ok()
}

fn looks_like_message_body(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.len() > 4_000 {
        return false;
    }
    // Reject obvious class / key noise from the archive.
    if t.starts_with("NS") || t.starts_with("__k") || t == "streamtyped" {
        return false;
    }
    t.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .count()
        >= t.len().saturating_mul(8) / 10
}

fn chat_db_path() -> PathBuf {
    dirs_messages().join("chat.db")
}

fn dirs_messages() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    PathBuf::from(home).join("Library/Messages")
}

/// Open chat.db read-only, live WAL (no immutable).
pub fn open_chat_db(path: &Path) -> Result<Connection, String> {
    // DECISION: mode=ro only — see module docs (opencode WAL lesson).
    let uri = format!("file:{}?mode=ro", path.display());
    Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| e.to_string())
}

pub fn fda_ok() -> bool {
    let path = chat_db_path();
    if !path.exists() {
        return false;
    }
    open_chat_db(&path).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfChat {
    pub chat_rowid: i64,
    pub handle_addr: String,
}

/// Self-chat predicate (verified against live schema when FDA allows):
/// ```sql
/// SELECT c.ROWID, h.id
/// FROM chat c
/// JOIN chat_handle_join chj ON chj.chat_id = c.ROWID
/// JOIN handle h ON h.ROWID = chj.handle_id
/// GROUP BY c.ROWID
/// HAVING COUNT(*) = 1 AND c.chat_identifier = h.id
/// ```
pub fn find_self_chats(con: &Connection) -> Result<Vec<SelfChat>, String> {
    let mut stmt = con
        .prepare(
            "SELECT c.ROWID, h.id \
             FROM chat c \
             JOIN chat_handle_join chj ON chj.chat_id = c.ROWID \
             JOIN handle h ON h.ROWID = chj.handle_id \
             GROUP BY c.ROWID \
             HAVING COUNT(*) = 1 AND c.chat_identifier = h.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SelfChat {
                chat_rowid: row.get(0)?,
                handle_addr: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMsg {
    pub rowid: i64,
    pub text: String,
    pub is_from_me: bool,
    pub chat_rowid: i64,
    pub handle_addr: Option<String>,
}

/// Resolve message body: prefer `text`, else decode `attributedBody`.
pub fn message_body(text: Option<String>, attributed: Option<Vec<u8>>) -> Option<String> {
    if let Some(t) = text {
        let t = t.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    attributed.as_deref().and_then(decode_attributed_body)
}

/// New messages in self-chats after `watermark`, oldest-first.
///
/// ```sql
/// SELECT m.ROWID, m.text, m.attributedBody, m.is_from_me, cmj.chat_id, h.id
/// FROM message m
/// JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
/// LEFT JOIN handle h ON h.ROWID = m.handle_id
/// WHERE m.ROWID > ? AND cmj.chat_id IN (…)
/// ORDER BY m.ROWID ASC
/// ```
pub fn poll_new(
    con: &Connection,
    self_chats: &[SelfChat],
    watermark: i64,
) -> Result<Vec<InboundMsg>, String> {
    if self_chats.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = self_chats
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT m.ROWID, m.text, m.attributedBody, m.is_from_me, cmj.chat_id, h.id \
         FROM message m \
         JOIN chat_message_join cmj ON cmj.message_id = m.ROWID \
         LEFT JOIN handle h ON h.ROWID = m.handle_id \
         WHERE m.ROWID > ? AND cmj.chat_id IN ({placeholders}) \
         ORDER BY m.ROWID ASC"
    );
    let mut stmt = con.prepare(&sql).map_err(|e| e.to_string())?;
    let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(1 + self_chats.len());
    params.push(watermark.into());
    for c in self_chats {
        params.push(c.chat_rowid.into());
    }
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params), |row| {
            let text: Option<String> = row.get(1)?;
            let attributed: Option<Vec<u8>> = row.get(2)?;
            let body = message_body(text, attributed).unwrap_or_default();
            Ok(InboundMsg {
                rowid: row.get(0)?,
                text: body,
                is_from_me: row.get::<_, i64>(3)? != 0,
                chat_rowid: row.get(4)?,
                handle_addr: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn max_rowid(con: &Connection) -> Result<i64, String> {
    con.query_row("SELECT COALESCE(MAX(ROWID), 0) FROM message", [], |r| {
        r.get(0)
    })
    .map_err(|e| e.to_string())
}

/// Whether an inbound row should be treated as a command.
pub fn is_command_candidate(
    msg: &InboundMsg,
    self_chats: &[SelfChat],
    own_handles: &[String],
) -> bool {
    if !msg.is_from_me {
        return false;
    }
    if msg.text.trim().is_empty() {
        return false;
    }
    if !self_chats.iter().any(|c| c.chat_rowid == msg.chat_rowid) {
        return false;
    }
    // Own-handle allowlist: if handle_addr is present, require it in the list.
    // Self-chat is_from_me rows often have NULL handle_id — still accept those.
    match &msg.handle_addr {
        Some(addr) if !own_handles.is_empty() => own_handles.iter().any(|h| h == addr),
        _ => true,
    }
}

/// Exact AppleScript shipped for outbound replies (plain text to self-handle).
pub fn applescript_send(handle: &str, body: &str) -> String {
    // Escape for AppleScript string literal.
    let escaped = body.replace('\\', "\\\\").replace('"', "\\\"");
    let handle_esc = handle.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"tell application "Messages"
  set targetService to 1st account whose service type = iMessage
  set targetBuddy to participant "{handle_esc}" of targetService
  send "{escaped}" to targetBuddy
end tell"#
    )
}

pub fn send_imessage(handle: &str, body: &str) -> Result<(), String> {
    let script = applescript_send(handle, body);
    let out = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("osascript failed: {}", err.trim()))
    }
}

/// Loop-guard helper: given prior watermark and post-send max ROWID, advance past own reply.
pub fn advance_watermark_past_send(prior: i64, max_after_send: i64) -> i64 {
    prior.max(max_after_send)
}

/// Pure poll step for tests: filter candidates, skip empty, respect watermark advance.
pub fn select_commands(
    msgs: &[InboundMsg],
    self_chats: &[SelfChat],
    own_handles: &[String],
    watermark: i64,
) -> (Vec<InboundMsg>, i64) {
    let mut next_wm = watermark;
    let mut cmds = Vec::new();
    for m in msgs {
        if m.rowid <= watermark {
            continue;
        }
        next_wm = next_wm.max(m.rowid);
        if is_command_candidate(m, self_chats, own_handles) {
            cmds.push(m.clone());
        }
    }
    (cmds, next_wm)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ImessageStatus {
    pub enabled: bool,
    pub approvals: bool,
    pub fda_ok: bool,
    pub detail: String,
}

#[tauri::command]
pub fn imessage_status(state: tauri::State<'_, Arc<AppState>>) -> ImessageStatus {
    let settings = state
        .settings
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let fda = fda_ok();
    let detail = if !settings.imessage_bridge_enabled {
        "off".into()
    } else if !fda {
        "needs Full Disk Access — off".into()
    } else {
        "on · polling self-chat".into()
    };
    ImessageStatus {
        enabled: settings.imessage_bridge_enabled,
        approvals: settings.imessage_approvals,
        fda_ok: fda,
        detail,
    }
}

fn run_gated(
    app: &AppHandle,
    state: &Arc<AppState>,
    text: &str,
    approvals: bool,
) -> Result<String, String> {
    let update = events::current_sessions(state);
    let rows = remote::board_from_wire(&update.sessions);
    let ids = events::ids_snapshot(state);
    let cmd = crate::grammar::parse(text);
    let action = crate::grammar::plan(&cmd, &rows, &ids);
    let action = gate_action(action, approvals)?;
    // TODO(M5): history line — `… via remote · imessage · HH:MM`
    remote::execute_action(app, state, action, &rows, LOGIN)
}

fn default_reply_handle(self_chats: &[SelfChat]) -> Option<String> {
    self_chats.first().map(|c| c.handle_addr.clone())
}

fn process_inbound(
    app: &AppHandle,
    state: &Arc<AppState>,
    con: &Connection,
    self_chats: &[SelfChat],
    own_handles: &[String],
    watermark: &mut i64,
    approvals: bool,
) {
    let msgs = match poll_new(con, self_chats, *watermark) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[imessage] poll: {e}");
            return;
        }
    };
    let (cmds, mut next_wm) = select_commands(&msgs, self_chats, own_handles, *watermark);
    *watermark = next_wm;

    let Some(reply_to) = default_reply_handle(self_chats) else {
        return;
    };

    for msg in cmds {
        let reply = match run_gated(app, state, &msg.text, approvals) {
            Ok(r) => r,
            Err(e) => e,
        };
        match send_imessage(&reply_to, &reply) {
            Ok(()) => {
                // DECISION: advance watermark past our own outbound so replies
                // are never re-parsed as commands (self-reply loop guard).
                match max_rowid(con) {
                    Ok(max_r) => {
                        *watermark = advance_watermark_past_send(*watermark, max_r);
                        next_wm = *watermark;
                    }
                    Err(e) => eprintln!("[imessage] max_rowid after send: {e}"),
                }
            }
            Err(e) => {
                eprintln!("[imessage] send failed (bridge degrades): {e}");
            }
        }
        let _ = next_wm;
    }
}

fn push_lines(update: &SessionsUpdate, prev: &HashMap<String, String>, settings: &crate::control::settings::Settings) -> Vec<String> {
    let mut lines = Vec::new();
    for s in &update.sessions {
        let prev_state = prev.get(&s.sid).map(|x| x.as_str()).unwrap_or("");
        if prev_state == s.state {
            continue;
        }
        let want = match s.state.as_str() {
            "done" => settings.imessage_push_done,
            "needs_you" => settings.imessage_push_needs_you,
            "stalled" => settings.imessage_push_stalled,
            _ => false,
        };
        if !want {
            continue;
        }
        let title = if s.title.is_empty() {
            s.sid.as_str()
        } else {
            s.title.as_str()
        };
        lines.push(format!("● {} · {} — {}", s.n, title, s.state));
    }
    lines
}

/// Start the poll + push threads. Safe when FDA is missing (degrades, never panics).
pub fn start(app: AppHandle, state: Arc<AppState>) {
    // Poll loop
    {
        let app = app.clone();
        let state = Arc::clone(&state);
        thread::spawn(move || {
            let mut watermark: i64 = 0;
            let mut initialized = false;
            let mut last_fda_err = Instant::now() - Duration::from_secs(60);
            loop {
                let settings = state
                    .settings
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone();
                if !settings.imessage_bridge_enabled {
                    initialized = false;
                    thread::sleep(Duration::from_secs(POLL_SECS));
                    continue;
                }
                let path = chat_db_path();
                let con = match open_chat_db(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        if last_fda_err.elapsed() >= Duration::from_secs(30) {
                            eprintln!(
                                "[imessage] bridge: off — needs Full Disk Access ({e})"
                            );
                            last_fda_err = Instant::now();
                        }
                        thread::sleep(Duration::from_secs(POLL_SECS));
                        continue;
                    }
                };
                let self_chats = match find_self_chats(&con) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[imessage] self-chat query: {e}");
                        thread::sleep(Duration::from_secs(POLL_SECS));
                        continue;
                    }
                };
                let own_handles: Vec<String> =
                    self_chats.iter().map(|c| c.handle_addr.clone()).collect();
                if !initialized {
                    match max_rowid(&con) {
                        Ok(m) => {
                            watermark = m;
                            initialized = true;
                            eprintln!(
                                "[imessage] polling · {} self-chat(s) · watermark={watermark}",
                                self_chats.len()
                            );
                        }
                        Err(e) => {
                            eprintln!("[imessage] init watermark: {e}");
                            thread::sleep(Duration::from_secs(POLL_SECS));
                            continue;
                        }
                    }
                }
                process_inbound(
                    &app,
                    &state,
                    &con,
                    &self_chats,
                    &own_handles,
                    &mut watermark,
                    settings.imessage_approvals,
                );
                thread::sleep(Duration::from_secs(POLL_SECS));
            }
        });
    }

    // Unsolicited pushes — same sessions:update bus as the phone page (no second watcher).
    {
        let state = Arc::clone(&state);
        let bus = Arc::clone(&state.remote_bus);
        thread::spawn(move || {
            let mut prev: HashMap<String, String> = HashMap::new();
            let mut last_push = Instant::now() - PUSH_MIN_GAP;
            let mut pending: Vec<String> = Vec::new();
            let (mut gen, _) = bus.snapshot();
            // Seed prev from current snapshot so we don't spam on start.
            {
                let update = events::current_sessions(&state);
                for s in &update.sessions {
                    prev.insert(s.sid.clone(), s.state.clone());
                }
            }
            loop {
                if let Some((g, payload)) = bus.wait_after(gen, Duration::from_secs(5)) {
                    gen = g;
                    if let Ok(update) = serde_json::from_str::<SessionsUpdate>(&payload) {
                        let settings = state
                            .settings
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .clone();
                        if settings.imessage_bridge_enabled {
                            let lines = push_lines(&update, &prev, &settings);
                            pending.extend(lines);
                        }
                        for s in &update.sessions {
                            prev.insert(s.sid.clone(), s.state.clone());
                        }
                    }
                }
                if pending.is_empty() || last_push.elapsed() < PUSH_MIN_GAP {
                    continue;
                }
                let settings = state
                    .settings
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone();
                if !settings.imessage_bridge_enabled {
                    pending.clear();
                    continue;
                }
                let path = chat_db_path();
                let Ok(con) = open_chat_db(&path) else {
                    continue;
                };
                let Ok(self_chats) = find_self_chats(&con) else {
                    continue;
                };
                let Some(handle) = default_reply_handle(&self_chats) else {
                    continue;
                };
                let batch: Vec<String> = pending.drain(..).collect();
                let body = batch.join("\n");
                if let Err(e) = send_imessage(&handle, &body) {
                    eprintln!("[imessage] push send failed: {e}");
                } else {
                    last_push = Instant::now();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_nanos_and_seconds() {
        // 2024-01-01 approx: unix 1704067200 → mac secs ≈ 725760000
        let mac_secs = 725_760_000i64;
        assert_eq!(mac_absolute_to_unix(mac_secs), 978_307_200 + mac_secs);
        let mac_nanos = mac_secs * 1_000_000_000;
        assert_eq!(mac_absolute_to_unix(mac_nanos), 978_307_200 + mac_secs);
    }

    #[test]
    fn gate_blocks_approve_and_deny_when_off() {
        let approve = Action::Approve {
            sid: "s".into(),
            letter: 'A',
        };
        let deny = Action::Deny {
            sid: "s".into(),
            n: 1,
            guidance: "no".into(),
        };
        assert_eq!(
            gate_action(approve.clone(), false).unwrap_err(),
            APPROVALS_DISABLED_REPLY
        );
        assert_eq!(
            gate_action(deny.clone(), false).unwrap_err(),
            APPROVALS_DISABLED_REPLY
        );
        assert!(gate_action(approve, true).is_ok());
        assert!(gate_action(Action::PrintStatus, false).is_ok());
        assert!(gate_action(
            Action::Prompt {
                sid: "s".into(),
                n: 1,
                text: "hi".into()
            },
            false
        )
        .is_ok());
        assert!(gate_action(
            Action::Nudge {
                sid: "s".into(),
                n: 1
            },
            false
        )
        .is_ok());
        assert!(gate_action(Action::Help, false).is_ok());
    }

    #[test]
    fn non_self_and_not_from_me_ignored() {
        let self_chats = vec![SelfChat {
            chat_rowid: 10,
            handle_addr: "+15551212".into(),
        }];
        let own = vec!["+15551212".to_string()];
        let other = InboundMsg {
            rowid: 5,
            text: "status".into(),
            is_from_me: false,
            chat_rowid: 10,
            handle_addr: Some("+1999".into()),
        };
        assert!(!is_command_candidate(&other, &self_chats, &own));
        let wrong_chat = InboundMsg {
            rowid: 6,
            text: "status".into(),
            is_from_me: true,
            chat_rowid: 99,
            handle_addr: None,
        };
        assert!(!is_command_candidate(&wrong_chat, &self_chats, &own));
        let ok = InboundMsg {
            rowid: 7,
            text: "status".into(),
            is_from_me: true,
            chat_rowid: 10,
            handle_addr: None,
        };
        assert!(is_command_candidate(&ok, &self_chats, &own));
    }

    #[test]
    fn loop_guard_advances_past_own_reply() {
        // Simulate: command at 100, we reply, Messages writes our reply as 101.
        let prior = 100i64;
        let max_after = 101i64;
        let wm = advance_watermark_past_send(prior, max_after);
        assert_eq!(wm, 101);
        // A fed-back reply text at rowid 101 would be ≤ watermark → not selected.
        let self_chats = vec![SelfChat {
            chat_rowid: 1,
            handle_addr: "me@x.com".into(),
        }];
        let reply_echo = InboundMsg {
            rowid: 101,
            text: "● 0 working · ● 1 done · ● 0 needs you".into(),
            is_from_me: true,
            chat_rowid: 1,
            handle_addr: None,
        };
        let (cmds, next) = select_commands(&[reply_echo], &self_chats, &["me@x.com".into()], wm);
        assert!(cmds.is_empty(), "own reply must not become a command");
        assert_eq!(next, wm);
    }

    #[test]
    fn attributed_body_extracts_nsstring() {
        // Minimal synthetic streamtyped-ish blob with NSString + length-prefixed body.
        let mut blob = b"streamtyped\0".to_vec();
        blob.extend_from_slice(&[0x01, 0x01]);
        blob.extend_from_slice(b"NSString");
        blob.push(0x00);
        let body = b"status";
        blob.push(body.len() as u8);
        blob.extend_from_slice(body);
        let got = decode_attributed_body(&blob);
        assert_eq!(got.as_deref(), Some("status"));
    }

    #[test]
    fn message_body_prefers_text_column() {
        assert_eq!(
            message_body(Some("  hello  ".into()), Some(b"NSString\x05nope".to_vec())).as_deref(),
            Some("hello")
        );
        assert_eq!(
            message_body(None, None),
            None
        );
    }

    #[test]
    fn applescript_contains_handle_and_body() {
        let s = applescript_send("me@example.com", r#"hi "there""#);
        assert!(s.contains("participant \"me@example.com\""));
        assert!(s.contains(r#"send "hi \"there\"" to targetBuddy"#));
        assert!(s.contains("tell application \"Messages\""));
    }
}
