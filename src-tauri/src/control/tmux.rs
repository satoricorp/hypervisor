//! Owned-tmux control — all calls use `tmux -L hypervisor`.
//! Never touch the user's default tmux server.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SOCKET: &str = "hypervisor";
static ID_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmux(args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("tmux");
    cmd.arg("-L").arg(SOCKET).args(args);
    let out = cmd
        .output()
        .map_err(|e| format!("tmux failed to start: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("tmux {:?} failed", args)
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn short_id() -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    let mixed = t
        ^ seq.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (std::process::id() as u64).wrapping_mul(0x85eb_ca6b);
    format!("{:08x}", mixed as u32)
}

/// Generate a UUID-shaped id without an extra crate.
fn session_uuid() -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let a = (t & 0xffff_ffff) as u32;
    let b = ((t >> 32) & 0xffff) as u16;
    let c = (0x4000 | (((t >> 48) as u16) & 0x0fff)) as u16; // version 4
    let d = (0x8000 | ((std::process::id() as u16) & 0x3fff)) as u16; // variant
    let e = (t.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 16) & 0xffff_ffff_ffff;
    format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[derive(Debug)]
pub struct Spawned {
    pub tmux_name: String,
    /// Known immediately for claude (`--session-id`); None for codex (poll).
    pub sid: Option<String>,
}

/// Detached tmux session running `shell_cmd` via `/bin/zsh -lic`.
pub fn new_detached(name: &str, cwd: &str, shell_cmd: &str) -> Result<(), String> {
    // DECISION: `/bin/zsh -lic` so nvm/.zshrc PATH resolves claude/codex.
    tmux(&[
        "new-session",
        "-d",
        "-s",
        name,
        "-c",
        cwd,
        "/bin/zsh",
        "-lic",
        shell_cmd,
    ])?;
    // Keep the pane around after the shell exits so spawn-health can capture
    // "command not found" (H3). Caller kills after the check.
    let _ = tmux(&["set-option", "-t", name, "remain-on-exit", "on"]);
    Ok(())
}

/// Respect-and-work-around gx: `~/.zshrc` exports ANTHROPIC_BASE_URL /
/// OPENAI_BASE_URL pointing at gx's local inference proxy (e.g.
/// http://localhost:8787). Since we spawn through `zsh -lic`, agents inherit
/// it. Probe the port first: if gx's proxy is up, keep routing through it
/// (capture/review); if it's down, unset the var so the agent falls back to the
/// real API instead of dying with `ConnectionRefused`. Runs in zsh (new_detached).
const GX_PROXY_GUARD: &str = "for _v in ANTHROPIC_BASE_URL OPENAI_BASE_URL; do _u=${(P)_v}; if [[ $_u == http://(localhost|127.0.0.1):* ]]; then _hp=${_u#*://}; _hp=${_hp%%/*}; nc -z -w1 ${_hp%%:*} ${_hp##*:} >/dev/null 2>&1 || unset $_v; fi; done; ";

/// Spawn a detached agent session. `name` overrides the tmux session name — used
/// so a worktree session's tmux id matches its `hv-<id>` branch; None → fresh.
pub fn spawn(
    harness: &str,
    model: &str,
    cwd: &str,
    name: Option<&str>,
) -> Result<Spawned, String> {
    let name = name
        .map(str::to_string)
        .unwrap_or_else(|| format!("hv-{}", short_id()));
    // DECISION: claude gets `--session-id` so owned.json can map before the
    // first prompt (jsonl only appears after the first user message).
    let (agent_cmd, sid) = match harness {
        "claude" | "claude code" => {
            let sid = session_uuid();
            (
                format!(
                    "claude --model {} --session-id {}",
                    shell_quote(model),
                    shell_quote(&sid)
                ),
                Some(sid),
            )
        }
        "codex" => (format!("codex -m {}", shell_quote(model)), None),
        "opencode" => {
            // Confirmed: `opencode --model provider/model` (also -m).
            (format!("opencode --model {}", shell_quote(model)), None)
        }
        "cursor" => {
            return Err("cursor is watch-only".into());
        }
        other => return Err(format!("unknown harness: {other}")),
    };
    let shell_cmd = format!("{GX_PROXY_GUARD}{agent_cmd}");
    new_detached(&name, cwd, &shell_cmd)?;
    Ok(Spawned {
        tmux_name: name,
        sid,
    })
}

/// Fresh `hv-<id8>` name for an adopted session.
pub fn next_hv_name() -> String {
    format!("hv-{}", short_id())
}

/// Send literal text then Enter (150ms apart so TUIs can compose first).
/// Claude Code boots in vim-like INSERT/manual mode inside tmux. Escape alone
/// drops into NORMAL mode where the first chars of a prompt are eaten as
/// motions ("Run Bash…" → "sh: …"). Re-enter INSERT with `i`, clear, then type.
pub fn send(target: &str, text: &str) -> Result<(), String> {
    // DECISION: Escape → i → C-u → literal → Enter. Dogfood finding 2026-07-10.
    let _ = tmux(&["send-keys", "-t", target, "Escape"]);
    thread::sleep(Duration::from_millis(60));
    let _ = tmux(&["send-keys", "-t", target, "Escape"]);
    thread::sleep(Duration::from_millis(60));
    let _ = tmux(&["send-keys", "-t", target, "i"]);
    thread::sleep(Duration::from_millis(60));
    let _ = tmux(&["send-keys", "-t", target, "C-u"]);
    thread::sleep(Duration::from_millis(60));
    tmux(&["send-keys", "-t", target, "-l", "--", text])?;
    thread::sleep(Duration::from_millis(150));
    tmux(&["send-keys", "-t", target, "Enter"])?;
    Ok(())
}

/// Send raw key names (e.g. `["1", "Enter"]`) — not literal `-l` mode.
pub fn send_keys(target: &str, keys: &[&str]) -> Result<(), String> {
    let mut args = vec!["send-keys", "-t", target];
    args.extend_from_slice(keys);
    tmux(&args)?;
    Ok(())
}

/// Capture the last `lines` of a pane (`-S` is negative, e.g. -25).
pub fn capture_pane(target: &str, lines: i32) -> Result<String, String> {
    let start = lines.to_string();
    tmux(&["capture-pane", "-p", "-t", target, "-S", &start])
}

pub fn kill(target: &str) -> Result<(), String> {
    tmux(&["kill-session", "-t", target])?;
    Ok(())
}

/// True if `target` exists on the hypervisor socket.
pub fn has_session(target: &str) -> bool {
    tmux(&["has-session", "-t", target]).is_ok()
}

/// True if the session is gone or its pane reports dead.
pub fn pane_dead(target: &str) -> bool {
    if !has_session(target) {
        return true;
    }
    match tmux(&["display-message", "-p", "-t", target, "#{pane_dead}"]) {
        Ok(s) => s.trim() == "1",
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn cursor_watch_only_and_tmux_socket_works() {
        let home = env::var("HOME").unwrap();
        let err = spawn("cursor", "x", &home, None).unwrap_err();
        assert!(err.contains("watch-only"), "{err}");

        let name = format!("hv-{}", short_id());
        tmux(&[
            "new-session",
            "-d",
            "-s",
            &name,
            "-c",
            &home,
            "sleep 60",
        ])
        .expect("tmux spawn");
        send(&name, "echo hv-ok").expect("send");
        kill(&name).expect("kill");
    }

    #[test]
    #[ignore]
    fn opencode_spawn_arm_builds_tmux_session() {
        if std::env::var("HV_LIVE").ok().as_deref() != Some("1") {
            eprintln!("skipping live test — set HV_LIVE=1 and run with --ignored");
            return;
        }
        let home = env::var("HOME").unwrap();
        let spawned = spawn("opencode", "opencode/big-pickle", &home, None).expect("opencode spawn");
        assert!(spawned.tmux_name.starts_with("hv-"));
        let _ = kill(&spawned.tmux_name);
    }
}
