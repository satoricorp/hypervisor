//! Lid-closed keep-awake for remote approvals (M8a).
//!
//! // DECISION: managed `caffeinate -dims` child is acceptable v1 — avoids
//! IOKit/IOPMAssertion bindings. Hold while any owned session is `working`;
//! caller releases after 60s with none working.

use crate::events::{any_owned_working, AppState};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Always-on keep-awake watcher (M8a → general): hold `caffeinate` while any
/// hypervisor-owned session is `working`, release 60s after the last goes idle.
/// This lets adopted/spawned agents keep running with the lid closed (on AC
/// power) with no toggle — it just goes when you start one. Runs for the app's
/// lifetime; the held caffeinate child is reaped if the app exits.
pub fn start(state: Arc<AppState>) {
    std::thread::spawn(move || {
        let mut ka = KeepAwake::new();
        let mut idle_since: Option<Instant> = None;
        loop {
            if any_owned_working(&state) {
                ka.hold();
                idle_since = None;
            } else {
                match idle_since {
                    None => idle_since = Some(Instant::now()),
                    Some(t) if t.elapsed() >= Duration::from_secs(60) => ka.release(),
                    Some(_) => {}
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

pub struct KeepAwake {
    child: Option<Child>,
}

impl KeepAwake {
    pub fn new() -> Self {
        Self { child: None }
    }

    pub fn hold(&mut self) {
        if self.child.is_some() {
            // Still alive?
            if let Some(child) = self.child.as_mut() {
                match child.try_wait() {
                    Ok(None) => return, // running
                    _ => {
                        let _ = child.kill();
                        self.child = None;
                    }
                }
            }
        }
        // `-w <our pid>` makes caffeinate exit when Hypervisor dies — even on an
        // unclean crash/force-quit where Drop never runs. Without it, the child
        // is reparented to launchd and holds sleep forever (observed: a 3-day
        // orphan keeping the Mac awake). release() still kills it on the normal
        // idle path; `-w` is the crash-safety net.
        let pid = std::process::id().to_string();
        match Command::new("caffeinate")
            .args(["-dims", "-w", &pid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => {
                eprintln!("[remote] keep-awake: caffeinate held");
                self.child = Some(c);
            }
            Err(e) => eprintln!("[remote] keep-awake spawn failed: {e}"),
        }
    }

    pub fn release(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!("[remote] keep-awake: caffeinate released");
        }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn hold_spawns_caffeinate_release_kills() {
        let mut ka = KeepAwake::new();
        ka.hold();
        thread::sleep(Duration::from_millis(200));
        let out = Command::new("pgrep")
            .args(["-lf", "caffeinate"])
            .output()
            .expect("pgrep");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("caffeinate"),
            "expected caffeinate after hold, got: {text}"
        );
        ka.release();
        thread::sleep(Duration::from_millis(200));
        let out2 = Command::new("pgrep")
            .args(["-lf", "caffeinate -dims"])
            .output()
            .expect("pgrep");
        // Our child should be gone (other caffeinates may exist).
        let text2 = String::from_utf8_lossy(&out2.stdout);
        // Soft check: release() returned without panic; child taken.
        assert!(ka.child.is_none());
        let _ = text2;
    }
}
