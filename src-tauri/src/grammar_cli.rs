//! Headless grammar executor for `hvscan cmd` (M7g).
//!
//! DECISION: hvscan cmd is preferred over a temporary tauri command — it's
//! scriptable without a running window. Loads owned.json from the app data
//! dir, scans adapters, detects approvals, assigns stable ids (sid-sorted
//! for deterministic CLI numbering across invocations), and routes through
//! the same approve/deny/tmux send paths as the desktop handlers.

use crate::approvals::{self, PendingApproval};
use crate::control::owned::{self, OwnedMap};
use crate::control::{opencode, tmux};
use crate::grammar::{self, Action, BoardRow};
use crate::registry::scan_sessions;
use crate::stable_ids::{self, StableIds};
use std::collections::HashMap;
use std::path::PathBuf;

fn app_data_dir() -> PathBuf {
    dirs_fallback()
}

fn dirs_fallback() -> PathBuf {
    // Match tauri identifier com.joe.hypervisor without depending on tauri in CLI.
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library/Application Support/com.joe.hypervisor");
    }
    PathBuf::from(".")
}

fn build_board() -> Result<(Vec<BoardRow>, StableIds, OwnedMap, HashMap<String, PendingApproval>), String> {
    let owned_path = app_data_dir().join("owned.json");
    let owned = owned::load(&owned_path);
    let sessions = scan_sessions(48.0, 64, None);

    let mut harness_by_sid = HashMap::new();
    let mut cwd_by_sid = HashMap::new();
    for s in &sessions {
        harness_by_sid.insert(s.sid.clone(), s.harness.clone());
        if s.harness == "opencode" && !s.cwd.is_empty() {
            cwd_by_sid.insert(s.sid.clone(), s.cwd.clone());
        }
    }

    let mut approvals = HashMap::new();
    approvals::detect_opencode(&cwd_by_sid, &mut approvals);
    approvals::detect_tmux(&owned, &harness_by_sid, &HashMap::new(), &mut approvals);

    let mut ids = StableIds::new();
    // Deterministic numbering for CLI: sorted sids.
    let mut sids: Vec<String> = sessions.iter().map(|s| s.sid.clone()).collect();
    for sid in owned.keys() {
        if !sids.contains(sid) {
            sids.push(sid.clone());
        }
    }
    ids.ensure_sids(sids.iter().cloned(), true);

    let mut live_idents = HashMap::new();
    for (sid, pending) in &approvals {
        let id = stable_ids::approval_identity(
            sid,
            &pending.source,
            pending.fingerprint.as_deref(),
            &pending.text,
        );
        live_idents.insert(sid.clone(), id);
    }
    ids.sync_approvals(&live_idents);

    // Board in scan order (mtime); n is stable.
    let rows: Vec<BoardRow> = sessions
        .iter()
        .map(|s| {
            let n = ids.number_of(&s.sid).unwrap_or(0);
            let approval = approvals.get(&s.sid).map(|a| a.text.clone());
            let state = if approval.is_some() {
                "needs_you".into()
            } else {
                s.state.clone()
            };
            BoardRow {
                n,
                sid: s.sid.clone(),
                title: if s.title.is_empty() {
                    s.sid.clone()
                } else {
                    s.title.clone()
                },
                state,
                letter: ids.letter_of_sid(&s.sid),
                approval,
            }
        })
        .collect();

    Ok((rows, ids, owned, approvals))
}

fn title_of(rows: &[BoardRow], sid: &str) -> String {
    rows.iter()
        .find(|r| r.sid == sid)
        .map(|r| r.title.clone())
        .unwrap_or_else(|| sid.to_string())
}

/// Run one grammar command; print result to stdout. Returns process exit code.
pub fn run_cmd(text: &str) -> i32 {
    let cmd = grammar::parse(text);
    let (rows, ids, owned, approvals) = match build_board() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("hvscan cmd: {e}");
            return 1;
        }
    };

    match grammar::plan(&cmd, &rows, &ids) {
        Action::PrintStatus => {
            println!("{}", grammar::format_status(&rows));
            0
        }
        Action::Help => {
            println!("{}", grammar::HELP);
            0
        }
        Action::Err(e) => {
            eprintln!("{e}");
            1
        }
        Action::Approve { sid, letter } => {
            let Some(pending) = approvals.get(&sid) else {
                eprintln!("no pending approval for {letter}");
                return 1;
            };
            let tmux_target = owned.get(&sid).map(|e| e.tmux.as_str());
            if let Err(e) = approvals::approve(pending, tmux_target) {
                eprintln!("approve failed: {e}");
                return 1;
            }
            let n = ids.number_of(&sid).unwrap_or(0);
            println!(
                "{}",
                grammar::echo_approved(letter, n, &title_of(&rows, &sid))
            );
            0
        }
        Action::Deny { sid, n, guidance } => {
            let Some(pending) = approvals.get(&sid) else {
                eprintln!("no pending approval on session {n}");
                return 1;
            };
            let tmux_target = owned.get(&sid).map(|e| e.tmux.as_str());
            if let Err(e) = approvals::deny(pending, &guidance, tmux_target) {
                eprintln!("deny failed: {e}");
                return 1;
            }
            println!("{}", grammar::echo_denied(n, &title_of(&rows, &sid)));
            0
        }
        Action::Prompt { sid, n, text } => {
            if let Err(e) = send_to(&sid, n, &text, &owned, &rows) {
                eprintln!("{e}");
                return 1;
            }
            println!("{}", grammar::echo_sent(n, &title_of(&rows, &sid)));
            0
        }
        Action::Nudge { sid, n } => {
            if let Err(e) = send_to(&sid, n, "continue", &owned, &rows) {
                eprintln!("{e}");
                return 1;
            }
            println!("{}", grammar::echo_sent(n, &title_of(&rows, &sid)));
            0
        }
    }
}

fn send_to(
    sid: &str,
    n: u32,
    text: &str,
    owned: &OwnedMap,
    _rows: &[BoardRow],
) -> Result<(), String> {
    if let Some(entry) = owned.get(sid) {
        return tmux::send(&entry.tmux, text);
    }
    let scanned = scan_sessions(48.0, 64, None);
    if let Some(s) = scanned.iter().find(|s| s.sid == sid) {
        if s.harness == "opencode" {
            return opencode::prompt_async(sid, &s.cwd, text);
        }
        return Err(format!("session {n} is not owned — adopt it first"));
    }
    Err(format!("session {n} not found"))
}
