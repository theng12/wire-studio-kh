//! settings.json — versioned, migrated, never reset by an update.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SETTINGS_FILE: &str = "settings.json";

/// The last answer Telegram itself gave for the saved token and channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelegramCheck {
    pub bot: String,
    pub chat: String,
    pub at: String,
}

/// Gateway's own answer to the last check: how much it publishes, and whether that read went stale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GatewayCheck {
    pub models: usize,
    pub stale: bool,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    /// Where Wire reads its models from, e.g. http://127.0.0.1:8787. Empty = never set.
    pub gateway_url: String,
    /// Set only when `/v1/models` answered this key with a model catalogue.
    pub gateway_verified: Option<GatewayCheck>,
    /// Gateway's own words when the last check failed; cleared on success.
    pub gateway_verify_error: String,
    /// The Telegram channel this Mac posts to, e.g. "@wire_smoke_test" or "-100…".
    pub telegram_chat: String,
    /// Telegram's own name for that channel, shown beside the saved destination.
    pub telegram_chat_title: String,
    /// system | light | dark
    pub appearance: String,
    /// Set only when Telegram answered getMe and getChat for this token and channel.
    pub telegram_verified: Option<TelegramCheck>,
    /// The provider's own words when the last check failed; cleared on success.
    pub telegram_verify_error: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            gateway_url: String::new(),
            gateway_verified: None,
            gateway_verify_error: String::new(),
            telegram_chat: String::new(),
            telegram_chat_title: String::new(),
            appearance: "system".into(),
            telegram_verified: None,
            telegram_verify_error: String::new(),
        }
    }
}

/// What the card shows: "configured" means saved and never checked, never "connected".
pub fn telegram_status(
    has_token: bool,
    chat: &str,
    chat_title: &str,
    verified: Option<&TelegramCheck>,
    verify_error: &str,
) -> (String, String) {
    if !has_token || chat.is_empty() {
        return ("not_configured".into(), String::new());
    }
    match verified {
        Some(check) => {
            let title = if chat_title.trim().is_empty() {
                check.chat.as_str()
            } else {
                chat_title.trim()
            };
            (
                "ready".into(),
                format!("connected · {} → {}", check.bot, title),
            )
        }
        None if !verify_error.is_empty() => ("failed".into(), verify_error.to_string()),
        None => (
            "configured".into(),
            "Saved, not verified yet. Replace the token to check the bot and channel.".into(),
        ),
    }
}

/// The same rule for Gateway: only a `/v1/models` answer may say "connected", and an empty
/// catalogue is a healthy connection — never an error and never a silent zero.
pub fn gateway_status(
    has_key: bool,
    url: &str,
    verified: Option<&GatewayCheck>,
    verify_error: &str,
) -> (String, String) {
    if !has_key || url.trim().is_empty() {
        return ("not_configured".into(), String::new());
    }
    match verified {
        Some(check) => {
            let mut reason = if check.models == 0 {
                "connected · no models published yet".to_string()
            } else {
                format!("connected · {} models available", check.models)
            };
            if check.stale {
                reason.push_str(" (catalogue is stale)");
            }
            ("ready".into(), reason)
        }
        None if !verify_error.is_empty() => ("failed".into(), verify_error.to_string()),
        None => (
            "configured".into(),
            "Saved, not checked yet. Press Check connection to ask Gateway.".into(),
        ),
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
        assert_eq!(load(&p).telegram_verified, None);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn gateway_status_reads_the_catalogue_never_the_health_route() {
        assert_eq!(gateway_status(false, "http://x", None, "").0, "not_configured");
        assert_eq!(gateway_status(true, "", None, "").0, "not_configured");
        assert_eq!(gateway_status(true, "http://x", None, "").0, "configured");
        assert_eq!(gateway_status(true, "http://x", None, "Invalid API key.").0, "failed");
        let empty = GatewayCheck { models: 0, stale: false, at: "t".into() };
        assert_eq!(
            gateway_status(true, "http://x", Some(&empty), ""),
            ("ready".into(), "connected · no models published yet".into())
        );
        let stale = GatewayCheck { models: 3, stale: true, at: "t".into() };
        assert_eq!(
            gateway_status(true, "http://x", Some(&stale), ""),
            ("ready".into(), "connected · 3 models available (catalogue is stale)".into())
        );
    }

    #[test]
    fn telegram_status_never_calls_saved_connected() {
        let saved = Settings::default();
        assert_eq!(telegram_status(false, "", "", None, "").0, "not_configured");
        assert_eq!(telegram_status(true, "@c", "", None, "").0, "configured");
        assert_eq!(telegram_status(true, "@c", "", None, "Unauthorized").0, "failed");
        let check = TelegramCheck {
            bot: "@wire_studio_kh_bot".into(),
            chat: "Wire Smoke Test".into(),
            at: "2026-11-21T15:00:00Z".into(),
        };
        let (state, reason) = telegram_status(true, "-1004483436341", "", Some(&check), "");
        assert_eq!(state, "ready");
        assert_eq!(reason, "connected · @wire_studio_kh_bot → Wire Smoke Test");
        let (_, reason) = telegram_status(
            true,
            "-1004483436341",
            "Saved Channel Name",
            Some(&check),
            "",
        );
        assert_eq!(reason, "connected · @wire_studio_kh_bot → Saved Channel Name");
        assert_eq!(saved.telegram_verify_error, "");
    }
}
