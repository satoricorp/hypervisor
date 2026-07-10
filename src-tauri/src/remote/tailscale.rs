//! Tailscale detection for M8a auth + Settings.

use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct TailscaleInfo {
    pub login: String,
    pub dns_name: String,
}

/// Probe `tailscale status --json`. Returns None if the CLI is missing/broken
/// or the daemon isn't up.
pub fn detect() -> Option<TailscaleInfo> {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let v: Value = serde_json::from_slice(&output.stdout).ok()?;
    let self_node = v.get("Self")?;
    let dns_name = self_node
        .get("DNSName")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let uid = self_node.get("UserID").and_then(|x| x.as_u64()).or_else(|| {
        self_node
            .get("UserID")
            .and_then(|x| x.as_i64())
            .map(|i| i as u64)
    })?;
    let profiles = v.get("UserProfiles")?;
    // UserProfiles keys are stringified ints.
    let profile = profiles
        .get(uid.to_string())
        .or_else(|| profiles.get(&uid.to_string()))?;
    let login = profile
        .get("LoginName")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    Some(TailscaleInfo { login, dns_name })
}

#[cfg(test)]
mod tests {
    #[test]
    fn detect_does_not_panic() {
        // May return None on this machine (Tailscale.app missing) — that's fine.
        let _ = super::detect();
    }
}
