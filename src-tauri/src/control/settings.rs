//! Persist app settings in app_data_dir/settings.json.
//! Local only — never touches harness dirs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SourceToggles {
    pub claude: bool,
    pub codex: bool,
    pub cursor: bool,
    pub opencode: bool,
}

impl Default for SourceToggles {
    fn default() -> Self {
        Self {
            claude: true,
            codex: true,
            cursor: true,
            opencode: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Settings {
    /// When true, tv_interrupt pauses YouTube on needs_you / stalled.
    #[serde(default = "default_true")]
    pub tv_pause_on_needs_you: bool,
    #[serde(default)]
    pub sources: SourceToggles,
    /// M8b: poll self-chat for grammar commands. Default off.
    #[serde(default)]
    pub imessage_bridge_enabled: bool,
    /// M8b: allow Approve/Deny over iMessage. Default OFF (identity is soft).
    #[serde(default)]
    pub imessage_approvals: bool,
    /// Unsolicited push opt-ins (default off). Batched ≤1 text / 30s.
    #[serde(default)]
    pub imessage_push_done: bool,
    #[serde(default)]
    pub imessage_push_needs_you: bool,
    #[serde(default)]
    pub imessage_push_stalled: bool,
    /// Anonymous feature-count analytics (PostHog). Default ON; disclosed once.
    #[serde(default = "default_true")]
    pub analytics: bool,
    /// Random UUID for PostHog distinct_id — never a hardware/user/host id.
    #[serde(default)]
    pub distinct_id: String,
    /// First-launch disclosure toast already shown.
    #[serde(default)]
    pub analytics_notice_shown: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tv_pause_on_needs_you: true,
            sources: SourceToggles::default(),
            imessage_bridge_enabled: false,
            imessage_approvals: false,
            imessage_push_done: false,
            imessage_push_needs_you: false,
            imessage_push_stalled: false,
            analytics: true,
            distinct_id: String::new(),
            analytics_notice_shown: false,
        }
    }
}

impl Settings {
    pub fn source_enabled(&self, harness: &str) -> bool {
        match harness {
            "claude code" | "claude" => self.sources.claude,
            "codex" => self.sources.codex,
            "cursor" => self.sources.cursor,
            "opencode" => self.sources.opencode,
            _ => true,
        }
    }

    /// Ensure distinct_id exists; returns true if settings should be re-saved.
    pub fn ensure_distinct_id(&mut self) -> bool {
        if self.distinct_id.is_empty() {
            self.distinct_id = crate::telemetry::new_distinct_id();
            true
        } else {
            false
        }
    }
}

pub fn load(path: &Path) -> Settings {
    let data = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Settings::default(),
    };
    match serde_json::from_str(&data) {
        Ok(s) => s,
        Err(_) => Settings::default(),
    }
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_save_roundtrip() {
        let dir = std::env::temp_dir().join(format!("hv-settings-{}", short_tmp()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        s.sources.codex = false;
        s.tv_pause_on_needs_you = false;
        s.imessage_bridge_enabled = true;
        s.imessage_approvals = false;
        s.analytics = false;
        s.distinct_id = "abc-123".into();
        save(&path, &s).unwrap();
        let loaded = load(&path);
        assert!(!loaded.sources.codex);
        assert!(!loaded.tv_pause_on_needs_you);
        assert!(loaded.imessage_bridge_enabled);
        assert!(!loaded.imessage_approvals);
        assert!(!loaded.analytics);
        assert_eq!(loaded.distinct_id, "abc-123");
        assert!(loaded.source_enabled("claude code"));
        assert!(!loaded.source_enabled("codex"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_imessage_fields_default_off() {
        let dir = std::env::temp_dir().join(format!("hv-settings-legacy-{}", short_tmp()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"tv_pause_on_needs_you":true,"sources":{"claude":true,"codex":true,"cursor":true,"opencode":true}}"#,
        )
        .unwrap();
        let loaded = load(&path);
        assert!(!loaded.imessage_bridge_enabled);
        assert!(!loaded.imessage_approvals);
        assert!(!loaded.imessage_push_done);
        assert!(loaded.analytics, "analytics defaults ON for legacy files");
        assert!(loaded.distinct_id.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    fn short_tmp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
