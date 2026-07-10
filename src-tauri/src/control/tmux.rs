//! Owned-tmux control — all calls use `tmux -L hypervisor`.
//! Never touch the user's default tmux server.

use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SOCKET: &str = "hypervisor";

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
    let mixed = t
        ^ (std::process::id() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    format!("{mixed:08x}")[..8].to_string()
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

/// Spawn a detached agent session.
pub fn spawn(harness: &str, model: &str, cwd: &str) -> Result<Spawned, String> {
    let name = format!("hv-{}", short_id());
    // DECISION: `/bin/zsh -lic` so nvm/.zshrc PATH resolves claude/codex.
    // DECISION: claude gets `--session-id` so owned.json can map before the
    // first prompt (jsonl only appears after the first user message).
    let (shell_cmd, sid) = match harness {
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
        "cursor" | "opencode" => {
            return Err("not wired until M2b".into());
        }
        other => return Err(format!("unknown harness: {other}")),
    };
    tmux(&[
        "new-session",
        "-d",
        "-s",
        &name,
        "-c",
        cwd,
        "/bin/zsh",
        "-lic",
        &shell_cmd,
    ])?;
    Ok(Spawned {
        tmux_name: name,
        sid,
    })
}

/// Send literal text then Enter (150ms apart so TUIs can compose first).
pub fn send(target: &str, text: &str) -> Result<(), String> {
    tmux(&["send-keys", "-t", target, "-l", "--", text])?;
    thread::sleep(Duration::from_millis(150));
    tmux(&["send-keys", "-t", target, "Enter"])?;
    Ok(())
}

pub fn kill(target: &str) -> Result<(), String> {
    tmux(&["kill-session", "-t", target])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn cursor_not_wired_and_tmux_socket_works() {
        let home = env::var("HOME").unwrap();
        let err = spawn("cursor", "x", &home).unwrap_err();
        assert!(err.contains("M2b"));
        let err = spawn("opencode", "x", &home).unwrap_err();
        assert!(err.contains("M2b"));

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
}
