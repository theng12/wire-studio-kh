//! settings.json — versioned, migrated, never reset by an update.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    /// The Telegram channel this Mac posts to, e.g. "@wire_smoke_test" or "-100…".
    pub telegram_chat: String,
    /// system | light | dark
    pub appearance: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            telegram_chat: String::new(),
            appearance: "system".into(),
        }
    }
}

pub fn path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTINGS_FILE)
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let raw = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, raw).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("wire-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = path_in(&dir);
        let mut s = Settings::default();
        assert_eq!(s.appearance, "system");
        s.telegram_chat = "@wire_smoke_test".into();
        save(&p, &s).unwrap();
        assert_eq!(load(&p).telegram_chat, "@wire_smoke_test");
        let _ = std::fs::remove_file(&p);
    }
}
