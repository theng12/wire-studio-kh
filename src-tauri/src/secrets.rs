//! One 0600 secrets file in the data dir. No Keychain, no second store.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

pub const SECRETS_FILE: &str = "app-secrets.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TelegramSecrets {
    pub bot_token: String,
    pub allowed_user_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Secrets {
    pub telegram: TelegramSecrets,
    pub operator_token: String,
}

impl Default for Secrets {
    fn default() -> Self {
        Self {
            telegram: TelegramSecrets::default(),
            operator_token: mint_token(),
        }
    }
}

impl Secrets {
    /// Mint on first use; an existing token is never replaced.
    pub fn ensure_operator_token(&mut self) -> &str {
        if self.operator_token.is_empty() {
            self.operator_token = mint_token();
        }
        &self.operator_token
    }
}

pub fn path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(SECRETS_FILE)
}

/// Missing or unreadable file is an empty store, not an error.
pub fn load(path: &Path) -> Secrets {
    let mut secrets: Secrets = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    secrets.ensure_operator_token();
    secrets
}

/// Atomic write: temp in the same dir, 0600, fsync, rename.
pub fn save(path: &Path, secrets: &Secrets) -> Result<(), String> {
    let mut secrets = secrets.clone();
    secrets.ensure_operator_token();
    let json = serde_json::to_vec_pretty(&secrets)
        .map_err(|error| format!("cannot encode secrets: {error}"))?;

    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create secrets dir {}: {error}", parent.display()))?;

    let mut nonce = [0u8; 8];
    OsRng.fill_bytes(&mut nonce);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("secrets");
    let temp = path.with_file_name(format!(
        ".{name}.tmp-{:x}-{}",
        u64::from_be_bytes(nonce),
        std::process::id()
    ));

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|error| format!("cannot create secrets temp file: {error}"))?;
    let write = (|| -> Result<(), std::io::Error> {
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(&json)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = write {
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot write secrets: {error}"));
    }
    drop(file);

    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot install secrets file: {error}"));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot set secrets file mode: {error}"))?;
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

fn mint_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let mut token = String::with_capacity(64);
    for byte in bytes {
        token.push_str(&format!("{byte:02x}"));
    }
    token
}

/// Show the last four characters only; never the whole secret.
pub fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 4 {
        return "…".into();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("…{tail}")
}

/// Replace every stored token value in a line with its mask.
pub fn redact_line(line: &str, secrets: &Secrets) -> String {
    let mut tokens = Vec::new();
    if !secrets.telegram.bot_token.is_empty() {
        tokens.push(secrets.telegram.bot_token.as_str());
    }
    if !secrets.operator_token.is_empty() {
        tokens.push(secrets.operator_token.as_str());
    }
    tokens.sort_by_key(|token| std::cmp::Reverse(token.len()));
    tokens.dedup();

    let mut redacted = line.to_string();
    for token in tokens {
        redacted = redacted.replace(token, &mask(token));
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut nonce = [0u8; 8];
        OsRng.fill_bytes(&mut nonce);
        let dir = std::env::temp_dir().join(format!(
            "wire-secrets-{tag}-{:x}-{}",
            u64::from_be_bytes(nonce),
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_and_operator_token() {
        let dir = temp_dir("roundtrip");
        let path = path_in(&dir);
        let mut secrets = Secrets::default();
        secrets.telegram.bot_token = "abcd-efgh".into();
        secrets.telegram.allowed_user_ids = vec![42, -7];
        save(&path, &secrets).unwrap();

        let loaded = load(&path);
        assert_eq!(loaded.telegram, secrets.telegram);
        assert_eq!(loaded.operator_token.len(), 64);
        assert!(loaded.operator_token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(load(&path).operator_token, loaded.operator_token);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_mode_is_0600() {
        let dir = temp_dir("mode");
        let path = path_in(&dir);
        save(&path, &Secrets::default()).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn masks_and_redacts() {
        assert_eq!(mask("abcd-efgh"), "…efgh");
        assert_eq!(mask("abc"), "…");
        let secrets = Secrets {
            telegram: TelegramSecrets {
                bot_token: "abcd-efgh".into(),
                allowed_user_ids: vec![],
            },
            operator_token: "op-9876".into(),
        };
        let line = "telegram abcd-efgh failed; operator op-9876";
        let redacted = redact_line(line, &secrets);
        assert_eq!(redacted, "telegram …efgh failed; operator …9876");
        assert!(!redacted.contains("abcd-efgh"));
        assert!(!redacted.contains("op-9876"));
    }
}
