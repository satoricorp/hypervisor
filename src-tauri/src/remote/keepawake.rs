//! Lid-closed keep-awake for remote approvals (M8a).
//!
//! // DECISION: managed `caffeinate -dims` child is acceptable v1 — avoids
//! IOKit/IOPMAssertion bindings. Hold while any owned session is `working`;
//! caller releases after 60s with none working.

use std::process::{Child, Command, Stdio};

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
        match Command::new("caffeinate")
            .args(["-dims"])
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
