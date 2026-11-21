//! Delivery adapter contract shared by every platform.

pub mod facebook;
pub mod telegram;
pub mod youtube;

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    #[default]
    None,
    Image,
    Video,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Media {
    pub kind: MediaKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PublishPayload {
    /// Platform destination: chat id, page id, channel handle, ...
    pub target: String,
    /// Already truncated by the base wrapper when one is in use.
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PublishOk {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// Original character count when the text was shortened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated_from: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub platform: String,
    pub max_text_length: usize,
    pub media_kinds: Vec<MediaKind>,
    pub honors_retry_after: bool,
    pub supports_edit: bool,
    pub supports_delete: bool,
    pub supports_metrics: bool,
    pub needs_public_image_url: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnState {
    Off,
    NotConfigured,
    Unsupported,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionState {
    pub state: ConnState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    AuthInvalid,
    PermissionDenied,
    RateLimited,
    ContentInvalid,
    MediaUnreachable,
    PlatformOutage,
    Network,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterError {
    pub kind: ErrorKind,
    pub message: String,
    /// Platform-supplied Retry-After; the scheduler prefers this over backoff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for AdapterError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Permanent,
    Transient,
    AuthPermission,
}

/// Auth and permission errors need a human; content errors cannot fix themselves.
/// Retry-After stays on the error so the scheduler can prefer it over backoff.
pub fn classify(err: &AdapterError) -> FailureClass {
    match err.kind {
        ErrorKind::AuthInvalid | ErrorKind::PermissionDenied => FailureClass::AuthPermission,
        ErrorKind::ContentInvalid => FailureClass::Permanent,
        ErrorKind::RateLimited
        | ErrorKind::MediaUnreachable
        | ErrorKind::PlatformOutage
        | ErrorKind::Network
        | ErrorKind::Unknown => FailureClass::Transient,
    }
}

/// Shorten to `max` characters, preferring a word boundary and marking the cut.
/// Returns (text, original_length); original_length is None when nothing was cut.
pub fn truncate_text(text: &str, max: usize) -> (String, Option<usize>) {
    let original = text.chars().count();
    if original <= max {
        return (text.to_string(), None);
    }
    if max == 0 {
        return (String::new(), Some(original));
    }
    let head_limit = max - 1; // leave room for the ellipsis
    let mut head: String = text.chars().take(head_limit).collect();
    let floor = head_limit * 7 / 10;
    if let Some(space_at) = head.rfind(' ') {
        let char_at = head[..space_at].chars().count();
        if char_at > floor {
            head.truncate(space_at);
        }
    }
    (format!("{}…", head.trim_end()), Some(original))
}

pub trait PlatformAdapter: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn connection_state(&self) -> BoxFuture<'_, Result<ConnectionState, AdapterError>>;
    fn publish(&self, payload: PublishPayload) -> BoxFuture<'_, Result<PublishOk, AdapterError>>;
}

/// 0.1.0 registry: Telegram is live; the two 0.3.0 skeletons report honestly.
/// shortcut: Secrets has no YouTube/Facebook records yet, so those credentials
/// are empty until the 0.3.0 secrets records are added.
pub fn adapters(secrets: &crate::secrets::Secrets) -> Vec<Box<dyn PlatformAdapter>> {
    vec![
        Box::new(telegram::TelegramAdapter::new(
            secrets.telegram.bot_token.clone(),
        )),
        Box::new(youtube::YouTubeAdapter::new(
            String::new(),
            String::new(),
            String::new(),
        )),
        Box::new(facebook::FacebookPageAdapter::new(
            String::new(),
            String::new(),
        )),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_reports_original_and_cuts_at_a_word() {
        let text = "one two three four five";
        let (short, original) = truncate_text(text, 15);
        assert_eq!(short, "one two three…");
        assert_eq!(original, Some(text.len()));

        let (unchanged, none) = truncate_text("short", 10);
        assert_eq!(unchanged, "short");
        assert_eq!(none, None);
    }

    #[test]
    fn classify_follows_the_taxonomy() {
        let err = |kind| AdapterError {
            kind,
            message: "x".into(),
            retry_after_seconds: None,
        };
        assert_eq!(classify(&err(ErrorKind::AuthInvalid)), FailureClass::AuthPermission);
        assert_eq!(classify(&err(ErrorKind::PermissionDenied)), FailureClass::AuthPermission);
        assert_eq!(classify(&err(ErrorKind::ContentInvalid)), FailureClass::Permanent);
        for kind in [
            ErrorKind::RateLimited,
            ErrorKind::MediaUnreachable,
            ErrorKind::PlatformOutage,
            ErrorKind::Network,
            ErrorKind::Unknown,
        ] {
            assert_eq!(classify(&err(kind)), FailureClass::Transient);
        }
    }
}
