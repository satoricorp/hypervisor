//! Pending-permission detection + approve/deny (M3).
//!
//! Detection lives here (registry/control layer), not in adapters.
//! opencode: poll GET /permission. tmux tier: capture-pane patterns derived
//! empirically from Claude Code's permission dialog (see Evidence in tasks/M3.md).

use crate::control::owned::OwnedMap;
use crate::control::{opencode, tmux};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum ApprovalSource {
    /// opencode HTTP permission request id (`per_…`).
    Opencode {
        request_id: String,
        /// Session cwd — required as `?directory=` on reply (live serve).
        directory: String,
    },
    /// Owned tmux pane (claude / codex).
    Tmux,
}

#[derive(Clone, Debug)]
pub struct PendingApproval {
    pub text: String,
    pub source: ApprovalSource,
    /// Pane content fingerprint for debounce (tmux only).
    pub fingerprint: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ToastEvent {
    pub html: String,
}

const TOOL_NAMES: &[&str] = &[
    "Bash",
    "Edit",
    "Write",
    "Read",
    "Fetch",
    "WebFetch",
    "NotebookEdit",
];

/// Extract `Tool(arg)` from a pane line, if present.
fn tool_call_on_line(line: &str) -> Option<String> {
    let t = line.trim();
    for name in TOOL_NAMES {
        let prefix = format!("{name}(");
        if let Some(rest) = t.strip_prefix(&prefix) {
            if let Some(end) = rest.find(')') {
                let arg = rest[..end].trim();
                if !arg.is_empty() {
                    return Some(format!("{name}({arg})"));
                }
            }
        }
    }
    None
}

fn has_yes_option(pane: &str) -> bool {
    pane.lines().any(|l| {
        let t = l.trim();
        // "❯ 1. Yes" / "1. Yes" / "> 1. Yes"
        let stripped = t.trim_start_matches(['❯', '>', ' ']);
        stripped.starts_with("1.") && stripped.contains("Yes")
    })
}

fn has_proceed(pane: &str) -> bool {
    pane.contains("Do you want to proceed?")
}

fn edit_target(pane: &str) -> Option<String> {
    // "Do you want to make this edit to main.rs?"
    const MARKER: &str = "Do you want to make this edit to ";
    for line in pane.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(MARKER) {
            let name = rest.trim().trim_end_matches('?').trim();
            if !name.is_empty() {
                return Some(format!("Edit({name})"));
            }
        }
    }
    None
}

/// Parse a Claude Code permission prompt from a tmux pane capture.
/// Returns human-readable approval text (e.g. `Bash(scripts/build.sh)`).
pub fn parse_claude_pane(pane: &str) -> Option<String> {
    let proceed = has_proceed(pane);
    let edit = edit_target(pane);
    let yes = has_yes_option(pane);

    if !proceed && edit.is_none() {
        return None;
    }
    // Prefer an explicit Tool(arg) line near the prompt.
    if proceed || yes {
        // Search upward-ish: any tool call line in the pane.
        for line in pane.lines().rev() {
            if let Some(call) = tool_call_on_line(line) {
                return Some(call);
            }
        }
    }
    if let Some(e) = edit {
        return Some(e);
    }
    if proceed && yes {
        return Some("permission request".into());
    }
    None
}

/// Codex TUI approval — best-effort. Returns None when the pane can't be
/// parsed reliably (documented in Evidence; allowed by M3 DoD #5).
pub fn parse_codex_pane(pane: &str) -> Option<String> {
    // DECISION: Codex approval UI is a fullscreen overlay with keymap actions
    // ("Approve the primary option", "Decline and provide corrective guidance")
    // rather than a stable numbered "Do you want to proceed?" block. Without a
    // live pane capture we cannot derive a reliable regex — leave codex out.
    let _ = pane;
    None
}

pub fn parse_tmux_pane(harness: &str, pane: &str) -> Option<String> {
    match harness {
        "claude code" | "claude" => parse_claude_pane(pane),
        "codex" => parse_codex_pane(pane),
        _ => None,
    }
}

fn fingerprint(pane: &str) -> String {
    pane.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Format an opencode PermissionRequest into sidebar approval text.
pub fn format_opencode_approval(permission: &str, patterns: &[String]) -> String {
    let pat = patterns.first().map(|s| s.as_str()).unwrap_or("");
    if pat.is_empty() {
        permission.to_string()
    } else if permission.eq_ignore_ascii_case("bash") {
        format!("Bash({pat})")
    } else {
        format!("{permission}({pat})")
    }
}

/// Approve via the appropriate control path.
pub fn approve(pending: &PendingApproval, tmux_target: Option<&str>) -> Result<(), String> {
    match &pending.source {
        ApprovalSource::Opencode {
            request_id,
            directory,
        } => opencode::permission_reply_in(request_id, "once", None, Some(directory)),
        ApprovalSource::Tmux => {
            let target = tmux_target.ok_or("no tmux target for approval")?;
            // Empirical: option 1 = Yes (default highlighted with ❯).
            tmux::send_keys(target, &["1", "Enter"])
        }
    }
}

/// Deny with guidance.
pub fn deny(
    pending: &PendingApproval,
    guidance: &str,
    tmux_target: Option<&str>,
) -> Result<(), String> {
    match &pending.source {
        ApprovalSource::Opencode {
            request_id,
            directory,
        } => {
            let msg = if guidance.trim().is_empty() {
                None
            } else {
                Some(guidance)
            };
            opencode::permission_reply_in(request_id, "reject", msg, Some(directory))
        }
        ApprovalSource::Tmux => {
            let target = tmux_target.ok_or("no tmux target for deny")?;
            // Empirical: option 3 = "No, and tell Claude what to do differently".
            tmux::send_keys(target, &["3", "Enter"])?;
            if !guidance.trim().is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(400));
                tmux::send(target, guidance)?;
            }
            Ok(())
        }
    }
}

/// Scan owned tmux sessions for permission prompts. Updates `out` in place.
/// Harness comes from owned.json v2; `harness_by_sid` is fallback for legacy
/// entries with an empty harness. Done-state sessions are still scanned —
/// a permission dialog can sit on a pane after the transcript goes idle.
pub fn detect_tmux(
    owned: &OwnedMap,
    harness_by_sid: &HashMap<String, String>,
    prev: &HashMap<String, PendingApproval>,
    out: &mut HashMap<String, PendingApproval>,
) {
    for (sid, entry) in owned {
        let harness = if !entry.harness.is_empty() {
            entry.harness.as_str()
        } else {
            match harness_by_sid.get(sid) {
                Some(h) => h.as_str(),
                None => continue,
            }
        };
        if harness != "claude code" && harness != "codex" {
            continue;
        }
        let pane = match tmux::capture_pane(&entry.tmux, -25) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let fp = fingerprint(&pane);
        if let Some(prev_a) = prev.get(sid) {
            if prev_a.fingerprint.as_deref() == Some(fp.as_str()) {
                out.insert(sid.clone(), prev_a.clone());
                continue;
            }
        }
        if let Some(text) = parse_tmux_pane(harness, &pane) {
            out.insert(
                sid.clone(),
                PendingApproval {
                    text,
                    source: ApprovalSource::Tmux,
                    fingerprint: Some(fp),
                },
            );
        }
    }
}

/// Merge opencode GET /permission results into `out`.
/// `cwd_by_sid` maps session id → directory for the required query param.
pub fn detect_opencode(
    cwd_by_sid: &HashMap<String, String>,
    out: &mut HashMap<String, PendingApproval>,
) {
    let mut dirs: Vec<String> = cwd_by_sid.values().cloned().collect();
    dirs.sort();
    dirs.dedup();
    let Ok(reqs) = opencode::list_permissions_for(Some(&dirs)) else {
        return;
    };
    for req in reqs {
        let directory = cwd_by_sid
            .get(&req.session_id)
            .cloned()
            .unwrap_or_default();
        let text = format_opencode_approval(&req.permission, &req.patterns);
        out.insert(
            req.session_id.clone(),
            PendingApproval {
                text,
                source: ApprovalSource::Opencode {
                    request_id: req.id,
                    directory,
                },
                fingerprint: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Empirical fixture from Claude Code permission dialog (GH #11380 +
    // binary strings for v2.1.206). Live OAuth expired on this machine —
    // see Evidence.
    const CLAUDE_PANE: &str = r#"
 Bash(scripts/build.sh)

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and don't ask again for bash scripts/build.sh commands in
   3. No, and tell Claude what to do differently (esc)
"#;

    const CLAUDE_EDIT_PANE: &str = r#"
 Edit file
 src/main.rs

 Do you want to make this edit to main.rs?
 ❯ 1. Yes
   2. Yes, and don't ask again for
   3. No, and tell Claude what to do differently (esc)
"#;

    #[test]
    fn parses_claude_bash_permission() {
        let text = parse_claude_pane(CLAUDE_PANE).expect("parse");
        assert_eq!(text, "Bash(scripts/build.sh)");
    }

    #[test]
    fn parses_claude_edit_permission() {
        let text = parse_claude_pane(CLAUDE_EDIT_PANE).expect("parse");
        assert_eq!(text, "Edit(main.rs)");
    }

    #[test]
    fn no_false_positive_on_normal_pane() {
        let pane = "❯ Run the shell command\n\n⏺ Done.\n";
        assert!(parse_claude_pane(pane).is_none());
    }

    #[test]
    fn opencode_approval_format() {
        assert_eq!(
            format_opencode_approval("bash", &["scripts/build.sh".into()]),
            "Bash(scripts/build.sh)"
        );
    }

    #[test]
    fn codex_returns_none() {
        assert!(parse_codex_pane("Approve the primary option").is_none());
    }
}
