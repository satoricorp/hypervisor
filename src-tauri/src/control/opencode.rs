//! Hypervisor-managed `opencode serve` child + HTTP prompt client.
//!
//! // DECISION: port 14096 (not 4096) so a user-started serve can't collide.
//! Binds 127.0.0.1 only — serve prints that it is unsecured without a password.
//!
//! Permissions (M3): GET /permission, POST /permission/{requestID}/reply.
//! SSE /event is optional polish — 2s poll passes DoD.

use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

// DECISION: 14096 not 4096 so a user-started serve can't collide.
const PORT: u16 = 14096;
const HOST: &str = "127.0.0.1";
const BASE: &str = "http://127.0.0.1:14096";

static SERVE: Mutex<Option<Child>> = Mutex::new(None);

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn healthy() -> bool {
    ureq::get(&format!("{BASE}/session"))
        .timeout(Duration::from_secs(2))
        .call()
        .map(|r| r.status() == 200)
        .unwrap_or(false)
}

fn resolve_opencode() -> Result<String, String> {
    // DECISION: resolve via zsh login so homebrew PATH works under Tauri.
    let out = Command::new("/bin/zsh")
        .args(["-lic", "which opencode"])
        .output()
        .map_err(|e| format!("which opencode: {e}"))?;
    if !out.status.success() {
        return Err("opencode not found on PATH".into());
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return Err("opencode not found on PATH".into());
    }
    Ok(path)
}

/// Spawn `opencode serve` lazily; no-op if already healthy.
pub fn ensure_serve() -> Result<(), String> {
    if healthy() {
        return Ok(());
    }

    let mut guard = SERVE.lock().map_err(|e| e.to_string())?;
    // Re-check after lock — another thread may have started it.
    if healthy() {
        return Ok(());
    }
    if let Some(child) = guard.as_mut() {
        if child.try_wait().ok().flatten().is_none() {
            // Still starting — wait for readiness.
            drop(guard);
            return wait_ready(Duration::from_secs(15));
        }
    }

    let bin = resolve_opencode()?;
    let mut child = Command::new(&bin)
        .args([
            "serve",
            "--port",
            &PORT.to_string(),
            "--hostname",
            HOST,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn opencode serve: {e}"))?;

    // Drain stderr so the pipe doesn't fill; watch stdout for the listen line.
    if let Some(stderr) = child.stderr.take() {
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                eprintln!("[opencode serve] {line}");
            }
        });
    }
    if let Some(stdout) = child.stdout.take() {
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                eprintln!("[opencode serve] {line}");
            }
        });
    }

    *guard = Some(child);
    drop(guard);
    wait_ready(Duration::from_secs(15))
}

fn wait_ready(budget: Duration) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < budget {
        if healthy() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(150));
    }
    Err(format!(
        "opencode serve did not become ready on {BASE} within {}s",
        budget.as_secs()
    ))
}

/// Fire-and-forget prompt via POST /session/{sid}/prompt_async.
pub fn prompt_async(sid: &str, cwd: &str, text: &str) -> Result<(), String> {
    ensure_serve()?;
    let url = format!(
        "{BASE}/session/{}/prompt_async?directory={}",
        percent_encode(sid),
        percent_encode(cwd)
    );
    let body = serde_json::json!({
        "parts": [{ "type": "text", "text": text }]
    });
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(10))
        .send_string(&body.to_string())
        .map_err(|e| format!("opencode prompt_async failed: {e}"))?;
    let status = resp.status();
    if !(200..300).contains(&status) {
        let msg = resp.into_string().unwrap_or_default();
        return Err(format!("opencode prompt_async HTTP {status}: {msg}"));
    }
    Ok(())
}

/// Pending permission from GET /permission (OpenAPI PermissionRequest).
#[derive(Deserialize, Clone, Debug)]
pub struct PermissionRequest {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub permission: String,
    #[serde(default)]
    pub patterns: Vec<String>,
}

/// List pending permissions. Bare GET returns [] unless scoped by directory
/// (verified against live serve) — pass known opencode session cwds.
pub fn list_permissions() -> Result<Vec<PermissionRequest>, String> {
    list_permissions_for(None)
}

pub fn list_permissions_for(directories: Option<&[String]>) -> Result<Vec<PermissionRequest>, String> {
    if !healthy() {
        return Ok(Vec::new());
    }
    let mut dirs: Vec<String> = match directories {
        Some(d) if !d.is_empty() => d.to_vec(),
        _ => vec![String::new()],
    };
    // Always try bare (docs say "all sessions") plus each cwd.
    if !dirs.iter().any(|d| d.is_empty()) {
        dirs.insert(0, String::new());
    }
    let mut by_id: std::collections::HashMap<String, PermissionRequest> =
        std::collections::HashMap::new();
    for dir in dirs {
        let url = if dir.is_empty() {
            format!("{BASE}/permission")
        } else {
            format!("{BASE}/permission?directory={}", percent_encode(&dir))
        };
        let resp = match ureq::get(&url).timeout(Duration::from_secs(3)).call() {
            Ok(r) => r,
            Err(_) => continue,
        };
        if resp.status() != 200 {
            continue;
        }
        let Ok(body) = resp.into_string() else { continue };
        let Ok(reqs) = serde_json::from_str::<Vec<PermissionRequest>>(&body) else {
            continue;
        };
        for req in reqs {
            by_id.insert(req.id.clone(), req);
        }
    }
    Ok(by_id.into_values().collect())
}

/// POST /permission/{requestID}/reply — body `{"reply":…,"message"?}`.
/// `directory` is required by the live server to locate the request.
pub fn permission_reply(
    request_id: &str,
    reply: &str,
    message: Option<&str>,
) -> Result<(), String> {
    permission_reply_in(request_id, reply, message, None)
}

pub fn permission_reply_in(
    request_id: &str,
    reply: &str,
    message: Option<&str>,
    directory: Option<&str>,
) -> Result<(), String> {
    ensure_serve()?;
    let url = match directory {
        Some(d) if !d.is_empty() => format!(
            "{BASE}/permission/{}/reply?directory={}",
            percent_encode(request_id),
            percent_encode(d)
        ),
        _ => format!("{BASE}/permission/{}/reply", percent_encode(request_id)),
    };
    let mut body = serde_json::json!({ "reply": reply });
    if let Some(m) = message {
        body["message"] = serde_json::Value::String(m.to_string());
    }
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(10))
        .send_string(&body.to_string())
        .map_err(|e| format!("opencode permission reply failed: {e}"))?;
    let status = resp.status();
    if !(200..300).contains(&status) {
        let msg = resp.into_string().unwrap_or_default();
        return Err(format!("opencode permission reply HTTP {status}: {msg}"));
    }
    Ok(())
}

/// Best-effort kill of the serve child (app exit).
pub fn shutdown() {
    if let Ok(mut guard) = SERVE.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::now_secs;
    use crate::control::tmux;
    use crate::registry::{scan_sessions, Harness};

    const IDLE_GUARD_S: f64 = 60.0;

    fn idle_guard_msg(idle: f64) -> String {
        format!(
            "active {idle:.0}s ago — it may still be open in another terminal. close it \
             there, or let it go idle, then prompt."
        )
    }

    #[test]
    fn idle_guard_message_shape() {
        let msg = idle_guard_msg(12.0);
        assert_eq!(
            msg,
            "active 12s ago — it may still be open in another terminal. close it \
             there, or let it go idle, then prompt."
        );
    }

    #[test]
    #[ignore]
    fn live_opencode_idle_guard_prompt_and_new() {
        if std::env::var("HV_LIVE").ok().as_deref() != Some("1") {
            eprintln!("skipping live test — set HV_LIVE=1 and run with --ignored");
            return;
        }
        let sessions = scan_sessions(48.0, 32, Some(Harness::Opencode));
        let Some(sess) = sessions.into_iter().next() else {
            eprintln!("OPENCODE_LIVE: no session in 48h window — skip");
            return;
        };
        let idle = now_secs() - sess.mtime;
        eprintln!(
            "OPENCODE_SIDEBAR: harness={} sid={} title={:?} model={} state={} cwd={} idle={:.0}s last_user={:?} last_assistant={:?}",
            sess.harness, sess.sid, sess.title, sess.model, sess.state, sess.cwd, idle,
            sess.last_user, sess.last_assistant
        );
        eprintln!("OPENCODE_CONTROL: api · background (non-owned)");

        if idle < IDLE_GUARD_S {
            let msg = idle_guard_msg(idle);
            eprintln!("OPENCODE_IDLE_GUARD_REFUSAL: {msg}");
            assert!(msg.contains("active") && msg.contains("then prompt"));
        } else {
            // Prove the hot-path refusal shape, then prompt while cold.
            let msg = idle_guard_msg(12.0);
            eprintln!("OPENCODE_IDLE_GUARD_REFUSAL: {msg}");
            prompt_async(&sess.sid, &sess.cwd, "hypervisor m2c ping — reply with: ok")
                .expect("prompt_async");
            eprintln!("OPENCODE_HTTP_PROMPT: ok sid={}", sess.sid);
        }

        // /new spawn + sid correlation
        let cwd = "/Users/joe/git/hypervisor";
        let spawn_time = now_secs();
        let spawned = tmux::spawn("opencode", "opencode/big-pickle", cwd)
            .expect("opencode /new spawn");
        eprintln!("OPENCODE_NEW_TMUX: {}", spawned.tmux_name);
        assert!(spawned.tmux_name.starts_with("hv-"));
        thread::sleep(Duration::from_secs(3));
        let _ = tmux::send(&spawned.tmux_name, "say hi in one word then stop");
        let sid = crate::control::owned::wait_for_sid("opencode", cwd, spawn_time);
        eprintln!("OPENCODE_NEW_CORRELATE: {:?}", sid);
        let _ = tmux::kill(&spawned.tmux_name);
    }
}
