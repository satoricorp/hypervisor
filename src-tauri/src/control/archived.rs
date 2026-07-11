//! Persist `{ sid → archived_at }` in app_data_dir/archived.json.
//! Local tombstones only — never touch harness transcript dirs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// sid → unix seconds when archived.
pub type ArchivedMap = HashMap<String, f64>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ArchivedWire {
    pub sid: String,
    pub title: String,
    pub harness: String,
    pub archived_at: f64,
}

pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn load(path: &Path) -> ArchivedMap {
    let data = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    match serde_json::from_str(&data) {
        Ok(m) => m,
        Err(_) => HashMap::new(),
    }
}

pub fn save(path: &Path, map: &ArchivedMap) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())
}

/// True if this sid should stay hidden given its current mtime.
pub fn is_hidden(map: &ArchivedMap, sid: &str, mtime: f64) -> bool {
    match map.get(sid) {
        Some(&at) => mtime <= at,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_save_roundtrip() {
        let dir = std::env::temp_dir().join(format!("hv-arch-{}", short_tmp()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("archived.json");
        let mut map = ArchivedMap::new();
        map.insert("abc".into(), 1_720_000_000.0);
        save(&path, &map).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.get("abc"), Some(&1_720_000_000.0));
        assert!(is_hidden(&loaded, "abc", 1_720_000_000.0));
        assert!(!is_hidden(&loaded, "abc", 1_720_000_001.0));
        assert!(!is_hidden(&loaded, "other", 0.0));
        let _ = fs::remove_dir_all(&dir);
    }

    fn short_tmp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
