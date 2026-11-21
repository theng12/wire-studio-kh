//! Check for Updates: read the pinned publisher file, verify the signed feed,
//! and answer with the five house states. It never downloads or installs.

use std::path::PathBuf;

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct UpdateState {
    /// not_checked | checking | up_to_date | available | failed
    pub state: String,
    pub version: Option<String>,
    pub detail: String,
}

fn publisher_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Library/KHUpdater/publisher.json")
}

/// Digits-only dot versions, 1..=6 parts, each 1..=9 digits. Missing parts are 0.
pub fn is_newer(feed: &str, installed: &str) -> Option<bool> {
    fn parts(v: &str) -> Option<Vec<u64>> {
        let p: Vec<&str> = v.split('.').collect();
        if p.is_empty() || p.len() > 6 {
            return None;
        }
        p.iter()
            .map(|s| {
                if s.is_empty() || s.len() > 9 || !s.bytes().all(|b| b.is_ascii_digit()) {
                    None
                } else {
                    s.parse::<u64>().ok()
                }
            })
            .collect()
    }
    let a = parts(feed)?;
    let b = parts(installed)?;
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = *a.get(i).unwrap_or(&0);
        let y = *b.get(i).unwrap_or(&0);
        if x != y {
            return Some(x > y);
        }
    }
    Some(false)
}

pub async fn check(installed: &str) -> UpdateState {
    let failed = |why: &str| UpdateState {
        state: "failed".into(),
        version: None,
        detail: why.to_string(),
    };

    let raw = match std::fs::read_to_string(publisher_path()) {
        Ok(s) => s,
        Err(_) => return failed("Not connected to an Update Studio on this Mac"),
    };
    let pub_doc: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return failed("The Update Studio file cannot be read"),
    };
    let feed_url = pub_doc.get("feedURL").and_then(|v| v.as_str()).unwrap_or("");
    let key_b64 = pub_doc.get("publicKey").and_then(|v| v.as_str()).unwrap_or("");
    if feed_url.is_empty() || key_b64.is_empty() {
        return failed("The Update Studio file has no feed address");
    }
    let key_bytes = match base64::engine::general_purpose::STANDARD.decode(key_b64) {
        Ok(b) if b.len() == 32 => b,
        _ => return failed("The Update Studio key cannot be read"),
    };
    let key = match VerifyingKey::from_bytes(&key_bytes.try_into().unwrap()) {
        Ok(k) => k,
        Err(_) => return failed("The Update Studio key cannot be read"),
    };

    let body: serde_json::Value = match reqwest::Client::new()
        .get(feed_url)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .and_then(|r| Ok(r))
    {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(v) => v,
            Err(_) => return failed("The update feed did not answer with a feed"),
        },
        Err(_) => return failed("The update feed is not reachable"),
    };

    let feed_b64 = body.get("feed").and_then(|v| v.as_str()).unwrap_or("");
    let sig_b64 = body.get("signature").and_then(|v| v.as_str()).unwrap_or("");
    let feed_bytes = match base64::engine::general_purpose::STANDARD.decode(feed_b64) {
        Ok(b) => b,
        Err(_) => return failed("The update feed is not signed correctly"),
    };
    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(sig_b64) {
        Ok(b) if b.len() == 64 => b,
        _ => return failed("The update feed is not signed correctly"),
    };
    let sig = Signature::from_bytes(&sig_bytes.try_into().unwrap());
    if key.verify(&feed_bytes, &sig).is_err() {
        return failed("The update feed failed its signature check");
    }

    let feed: serde_json::Value = match serde_json::from_slice(&feed_bytes) {
        Ok(v) => v,
        Err(_) => return failed("The update feed cannot be read"),
    };
    if feed.get("schema").and_then(|v| v.as_i64()) != Some(1) {
        return failed("The update feed is a newer format than this app understands");
    }
    let bundle_id = "com.terranash.wirestudiokh";
    let app = feed
        .get("apps")
        .and_then(|a| a.as_object())
        .and_then(|m| {
            m.values()
                .find(|e| e.get("bundleID").and_then(|b| b.as_str()) == Some(bundle_id))
        });
    let app = match app {
        Some(a) => a,
        None => {
            return UpdateState {
                state: "up_to_date".into(),
                version: Some(installed.into()),
                detail: "Not offered by this Update Studio".into(),
            }
        }
    };
    let version = app.get("version").and_then(|v| v.as_str()).unwrap_or("");
    match is_newer(version, installed) {
        Some(true) => UpdateState {
            state: "available".into(),
            version: Some(version.into()),
            detail: format!("{version} is available"),
        },
        Some(false) => UpdateState {
            state: "up_to_date".into(),
            version: Some(installed.into()),
            detail: "up to date".into(),
        },
        None => failed("The update feed has a version this app cannot read"),
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn version_rule() {
        assert_eq!(is_newer("1.0.1", "1.0.0"), Some(true));
        assert_eq!(is_newer("1.0", "1.0.0"), Some(false));
        assert_eq!(is_newer("1.0.0.0.0.1", "1.0"), Some(true));
        assert_eq!(is_newer("1..0", "1.0"), None);
        assert_eq!(is_newer("v1", "1.0"), None);
    }
}
