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
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tv_pause_on_needs_you: true,
            sources: SourceToggles::default(),
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
        save(&path, &s).unwrap();
        let loaded = load(&path);
        assert!(!loaded.sources.codex);
        assert!(!loaded.tv_pause_on_needs_you);
        assert!(loaded.source_enabled("claude code"));
        assert!(!loaded.source_enabled("codex"));
        let _ = fs::remove_dir_all(&dir);
    }

    fn short_tmp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
