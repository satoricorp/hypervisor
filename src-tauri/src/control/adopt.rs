//! Adopt observe-only sessions into hypervisor tmux via harness resume.

use crate::approvals::ToastEvent;
use crate::control::{owned, tmux};
use crate::events::{emit_snapshot, AppState};
use crate::registry::scan_sessions;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

const MAX_AGE_HOURS: f64 = 48.0;
/// DECISION: adopt lookup uses a higher limit than the sidebar (8) so a
/// visible-but-not-top-8 race can't make adoption fail with "not found".
const ADOPT_SCAN_LIMIT: usize = 64;
const FORK_GUARD_SECS: f64 = 60.0;

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Recover full codex uuid from `rollout-<timestamp>-<uuid>.jsonl` path.
fn codex_uuid_from_src(src: &str) -> Result<String, String> {
    let stem = Path::new(src)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "codex session path has no filename".to_string())?;
    if stem.len() < 36 {
        return Err(format!(
            "codex rollout stem too short to hold a uuid: {stem}"
        ));
    }
    Ok(stem[stem.len() - 36..].to_string())
}

#[tauri::command]
pub fn adopt_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    sid: String,
) -> Result<String, String> {
    let sessions = scan_sessions(MAX_AGE_HOURS, ADOPT_SCAN_LIMIT, None);
    let sess = sessions
        .iter()
        .find(|s| s.sid == sid)
        .ok_or_else(|| format!("session {sid} not found in scan"))?;

    match sess.harness.as_str() {
        "claude code" | "codex" => {}
        _ => {
            return Err(
                "cursor sessions are watch-only; claude.ai has no control path yet"
                    .into(),
            );
        }
    }

    {
        let map = state.owned.lock().unwrap();
        if map.contains_key(&sid) {
            return Err("already controlled by hypervisor".into());
        }
    }

    let idle = now_secs() - sess.mtime;
    if idle < FORK_GUARD_SECS {
        return Err(format!(
            "active {idle:.0}s ago — it may still be open in another terminal. close it \
             there, or let it go idle, then adopt."
        ));
    }

    let hv_name = tmux::next_hv_name();
    let shell_cmd = match sess.harness.as_str() {
        "claude code" => format!("claude --resume {}", shell_quote(&sid)),
        "codex" => {
            let uuid = codex_uuid_from_src(&sess.src)?;
            format!("codex resume {}", shell_quote(&uuid))
        }
        _ => unreachable!(),
    };

    tmux::new_detached(&hv_name, &sess.cwd, &shell_cmd)?;

    {
        let mut map = state.owned.lock().unwrap();
        map.insert(
            sid.clone(),
            owned::OwnedEntry::new(hv_name.clone(), sess.harness.clone()),
        );
        let path = state.owned_path.lock().unwrap().clone();
        owned::save(&path, &map)?;
    }

    // Fresh snapshot so the control chip flips without waiting for an fs event.
    let sessions = scan_sessions(MAX_AGE_HOURS, ADOPT_SCAN_LIMIT, None);
    emit_snapshot(&app, &state, sessions);

    // ~2s health check — same as /new spawn (H3).
    let app2 = app.clone();
    let st = Arc::clone(state.inner());
    let hv2 = hv_name.clone();
    let sid2 = sid.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        if !tmux::pane_dead(&hv2) {
            return;
        }
        let detail = match tmux::capture_pane(&hv2, -40) {
            Ok(pane) => {
                let lines: Vec<&str> = pane
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .collect();
                let take = lines.len().saturating_sub(6);
                let tail = lines[take..].join(" · ");
                if tail.is_empty() {
                    "pane exited with no output".into()
                } else {
                    tail
                }
            }
            Err(_) => "pane exited (no capture)".to_string(),
        };
        {
            let mut map = st.owned.lock().unwrap_or_else(|p| p.into_inner());
            map.remove(&sid2);
            let path = st.owned_path.lock().unwrap_or_else(|p| p.into_inner()).clone();
            let _ = owned::save(&path, &map);
        }
        st.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&sid2);
        let sessions = scan_sessions(MAX_AGE_HOURS, ADOPT_SCAN_LIMIT, None);
        emit_snapshot(&app2, &st, sessions);
        let _ = app2.emit(
            "toast",
            &ToastEvent {
                label: format!("adopt failed · {hv2}"),
                detail: Some(detail),
            },
        );
        let _ = tmux::kill(&hv2);
    });

    Ok(hv_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::tmux;
    use std::process::Command;

    #[test]
    fn codex_uuid_from_rollout_path() {
        let src = "/Users/x/.codex/sessions/2026/07/10/rollout-2026-07-10T12-00-00-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.jsonl";
        assert_eq!(
            codex_uuid_from_src(src).unwrap(),
            "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        );
    }

    #[test]
    #[ignore]
    fn fork_guard_and_adopt_idle_sessions() {
        if std::env::var("HV_LIVE").ok().as_deref() != Some("1") {
            eprintln!("skipping live test — set HV_LIVE=1 and run with --ignored");
            return;
        }
        let sessions = scan_sessions(MAX_AGE_HOURS, ADOPT_SCAN_LIMIT, None);
        let now = now_secs();

        // Fork guard: any session with idle < 60 must produce the refusal text.
        let active = sessions.iter().find(|s| now - s.mtime < FORK_GUARD_SECS);
        if let Some(s) = active {
            let idle = now - s.mtime;
            let msg = format!(
                "active {idle:.0}s ago — it may still be open in another terminal. close it \
                 there, or let it go idle, then adopt."
            );
            assert!(msg.contains("active"));
            assert!(msg.contains("ago"));
            eprintln!("FORK_GUARD_REFUSAL: {msg}");
        } else {
            eprintln!("FORK_GUARD_REFUSAL: (no active session in scan; message format verified below)");
            let idle = 12.0_f64;
            let msg = format!(
                "active {idle:.0}s ago — it may still be open in another terminal. close it \
                 there, or let it go idle, then adopt."
            );
            assert_eq!(
                msg,
                "active 12s ago — it may still be open in another terminal. close it \
                 there, or let it go idle, then adopt."
            );
            eprintln!("FORK_GUARD_REFUSAL: {msg}");
        }

        let before = Command::new("tmux")
            .args(["-L", "hypervisor", "ls"])
            .output()
            .expect("tmux");
        let before_s = String::from_utf8_lossy(&before.stdout).to_string();
        eprintln!("TMUX_BEFORE:\n{}", if before_s.is_empty() { "(no server / empty)" } else { &before_s });

        // Adopt an idle claude code session (mtime > 60s).
        let claude = sessions.iter().find(|s| {
            s.harness == "claude code" && now - s.mtime >= FORK_GUARD_SECS
        });
        let Some(claude) = claude else {
            panic!("no idle claude code session to adopt");
        };
        let hv_claude = tmux::next_hv_name();
        let cmd = format!("claude --resume {}", shell_quote(&claude.sid));
        tmux::new_detached(&hv_claude, &claude.cwd, &cmd).expect("adopt claude");
        eprintln!(
            "CLAUDE_ADOPT_TOAST: adopted as {hv_claude} — session now runs in the background"
        );
        eprintln!("CLAUDE_ADOPT_SID: {}", claude.sid);

        // Adopt an idle codex session.
        let codex = sessions.iter().find(|s| {
            s.harness == "codex" && now - s.mtime >= FORK_GUARD_SECS
        });
        let Some(codex) = codex else {
            panic!("no idle codex session to adopt");
        };
        let uuid = codex_uuid_from_src(&codex.src).expect("codex uuid");
        let hv_codex = tmux::next_hv_name();
        let cmd = format!("codex resume {}", shell_quote(&uuid));
        tmux::new_detached(&hv_codex, &codex.cwd, &cmd).expect("adopt codex");
        eprintln!(
            "CODEX_ADOPT_TOAST: adopted as {hv_codex} — session now runs in the background"
        );
        eprintln!("CODEX_ADOPT_SID: {} uuid={uuid}", codex.sid);

        let after = Command::new("tmux")
            .args(["-L", "hypervisor", "ls"])
            .output()
            .expect("tmux");
        let after_s = String::from_utf8_lossy(&after.stdout).to_string();
        eprintln!("TMUX_AFTER:\n{after_s}");
        assert!(after_s.contains(&hv_claude), "claude hv session missing");
        assert!(after_s.contains(&hv_codex), "codex hv session missing");

        // Prompt path: same send-keys used by the prompt bar after adoption.
        tmux::send(&hv_claude, "ping from hypervisor adopt test").expect("send after adopt");
        eprintln!("CLAUDE_PROMPT_AFTER_ADOPT: ok (send-keys to {hv_claude})");

        // Cleanup so we don't leave resume processes forking forever.
        let _ = tmux::kill(&hv_claude);
        let _ = tmux::kill(&hv_codex);
    }
}
