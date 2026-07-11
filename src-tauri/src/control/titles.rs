//! Persist `{ sid → custom_title }` in app_data_dir/titles.json.
//! Local overrides only — never touch harness transcript dirs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// sid → user-chosen title.
pub type TitlesMap = HashMap<String, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TitlesWire {
    pub sid: String,
    pub title: String,
}

pub fn load(path: &Path) -> TitlesMap {
    let data = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    match serde_json::from_str(&data) {
        Ok(m) => m,
        Err(_) => HashMap::new(),
    }
}

pub fn save(path: &Path, map: &TitlesMap) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_save_roundtrip() {
        let dir = std::env::temp_dir().join(format!("hv-titles-{}", short_tmp()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("titles.json");
        let mut map = TitlesMap::new();
        map.insert("abc".into(), "payments spike".into());
        save(&path, &map).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.get("abc").map(|s| s.as_str()), Some("payments spike"));
        map.remove("abc");
        save(&path, &map).unwrap();
        assert!(load(&path).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    fn short_tmp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
