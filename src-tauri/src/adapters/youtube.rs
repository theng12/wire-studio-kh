//! YouTube Data API v3 adapter: OAuth refresh + resumable video upload, no SDK.
//! No token or client secret ever reaches a log line or error text.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, CONTENT_RANGE, CONTENT_TYPE, LOCATION, RETRY_AFTER};
use reqwest::StatusCode;
use serde_json::Value;

use super::{
    truncate_text, AdapterError, BoxFuture, Capabilities, ConnState, ConnectionState, ErrorKind,
    MediaKind, PlatformAdapter, PublishOk, PublishPayload,
};

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const UPLOAD_INIT_URL: &str =
    "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status";
const CHANNELS_URL: &str = "https://www.googleapis.com/youtube/v3/channels?part=id&mine=true";
const MAX_TEXT_LENGTH: usize = 5000;
const TITLE_MAX_LENGTH: usize = 100;
/// Resumable chunks must be multiples of 256 KiB; 8 MiB keeps memory bounded
/// on multi-GB videos (one chunk in RAM, never the whole file).
const CHUNK_SIZE: usize = 8 * 1024 * 1024;
const TEXT_ONLY_MESSAGE: &str = "YouTube needs a video; this post has text only";
const RECONNECT_MESSAGE: &str = "Reconnect YouTube";
/// Refresh early enough that an in-flight publish never uses a dying token.
const TOKEN_EXPIRY_MARGIN: Duration = Duration::from_secs(60);

pub struct YouTubeAdapter {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub http: reqwest::Client,
    /// Cached OAuth access token and its expiry; never logged.
    access_token_cache: Mutex<Option<CachedToken>>,
}

#[derive(Clone)]
struct CachedToken {
    token: String,
    expires_at: Instant,
}

impl std::fmt::Debug for YouTubeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("YouTubeAdapter")
            .field("client_secret", &"[redacted]")
            .field("refresh_token", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl YouTubeAdapter {
    pub fn new(client_id: String, client_secret: String, refresh_token: String) -> Self {
        Self {
            client_id,
            client_secret,
            refresh_token,
            http: reqwest::Client::new(),
            access_token_cache: Mutex::new(None),
        }
    }

    /// reqwest errors can echo the request, which carries the client secret.
    fn scrub(&self, text: &str) -> String {
        let mut secrets = [self.client_secret.as_str(), self.refresh_token.as_str()];
        secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
        let mut redacted = text.to_string();
        for secret in secrets {
            if !secret.is_empty() {
                redacted = redacted.replace(secret, "[redacted]");
            }
        }
        redacted
    }

    fn description(body: &str) -> String {
        let fallback: String = body.chars().take(200).collect();
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .filter(|text| !text.is_empty())
            .unwrap_or(if fallback.is_empty() {
                "no response body".to_string()
            } else {
                fallback
            })
    }

    fn map_status(&self, status: StatusCode, headers: &HeaderMap, body: &str) -> AdapterError {
        let kind = match status.as_u16() {
            401 => ErrorKind::AuthInvalid,
            403 => ErrorKind::PermissionDenied,
            429 => ErrorKind::RateLimited,
            400 | 422 => ErrorKind::ContentInvalid,
            500..=599 => ErrorKind::PlatformOutage,
            _ => ErrorKind::Unknown,
        };
        let retry_after_seconds = if kind == ErrorKind::RateLimited {
            headers
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
        } else {
            None
        };
        AdapterError {
            kind,
            message: self.scrub(&format!("YouTube HTTP {status}: {}", Self::description(body))),
            retry_after_seconds,
        }
    }

    fn map_transport(&self, context: &str, error: impl std::fmt::Display) -> AdapterError {
        AdapterError {
            kind: ErrorKind::Network,
            message: self.scrub(&format!("{context}: {error}")),
            retry_after_seconds: None,
        }
    }

    fn unknown(&self, context: &str, body: &str) -> AdapterError {
        AdapterError {
            kind: ErrorKind::Unknown,
            message: self.scrub(&format!("{context}: {}", Self::description(body))),
            retry_after_seconds: None,
        }
    }

    /// A plain reason for a Failed connection state; auth problems point at reconnect.
    fn failed_reason(error: &AdapterError) -> String {
        if matches!(
            error.kind,
            ErrorKind::AuthInvalid | ErrorKind::PermissionDenied
        ) {
            RECONNECT_MESSAGE.to_string()
        } else {
            error.message.clone()
        }
    }

    /// A valid access token: cached while it outlives the expiry margin,
    /// otherwise a fresh refresh grant. The token is for headers, never logs.
    async fn access_token(&self) -> Result<String, AdapterError> {
        if let Ok(cache) = self.access_token_cache.lock() {
            if let Some(cached) = cache.as_ref() {
                if cached.expires_at > Instant::now() + TOKEN_EXPIRY_MARGIN {
                    return Ok(cached.token.clone());
                }
            }
        }
        self.refresh_access_token().await
    }

    /// OAuth refresh-token grant. The returned access token is for the caller,
    /// never for a log line.
    pub async fn refresh_access_token(&self) -> Result<String, AdapterError> {
        let form = [
            format!("client_id={}", form_value(&self.client_id)),
            format!("client_secret={}", form_value(&self.client_secret)),
            format!("refresh_token={}", form_value(&self.refresh_token)),
            "grant_type=refresh_token".to_string(),
        ]
        .join("&");
        let response = self
            .http
            .post(TOKEN_URL)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form)
            .send()
            .await
            .map_err(|error| self.map_transport("token refresh", error))?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let mut error = self.map_status(status, &headers, &body);
            // From the token endpoint a 400 is a rejected refresh grant
            // (invalid_grant), not bad content: it needs a reconnect.
            if status.as_u16() == 400 {
                error.kind = ErrorKind::AuthInvalid;
                error.message =
                    self.scrub(&format!("token refresh rejected: {}", Self::description(&body)));
            }
            return Err(error);
        }
        let value: Value = serde_json::from_str(&body)
            .map_err(|_| self.unknown("token refresh", &body))?;
        let token = value
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| self.unknown("token refresh", &body))?;
        let expires_in = value
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(3600);
        if let Ok(mut cache) = self.access_token_cache.lock() {
            *cache = Some(CachedToken {
                token: token.clone(),
                expires_at: Instant::now() + Duration::from_secs(expires_in),
            });
        }
        Ok(token)
    }
}

/// reqwest's `form` feature is off; encode the four fixed fields here.
fn form_value(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// MIME from the file extension; YouTube accepts these for uploads.
fn mime_for(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "mpeg" | "mpeg4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

/// Read until the buffer is full or the file ends; returns bytes read.
fn read_chunk(file: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// A 308 response reports progress as `Range: bytes=0-N`; take the end offset.
fn parse_range(headers: &HeaderMap) -> Option<u64> {
    let value = headers.get(reqwest::header::RANGE)?.to_str().ok()?;
    value.rsplit('-').next()?.trim().parse::<u64>().ok()
}

impl PlatformAdapter for YouTubeAdapter {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: "youtube".into(),
            max_text_length: MAX_TEXT_LENGTH,
            media_kinds: vec![MediaKind::Video, MediaKind::Image],
            honors_retry_after: true,
            supports_edit: false,
            supports_delete: true,
            supports_metrics: true,
            needs_public_image_url: false,
        }
    }

    fn connection_state(&self) -> BoxFuture<'_, Result<ConnectionState, AdapterError>> {
        Box::pin(async move {
            if self.client_id.trim().is_empty() {
                return Ok(ConnectionState {
                    state: ConnState::NotConfigured,
                    reason: Some("no client id".into()),
                });
            }
            if self.refresh_token.trim().is_empty() {
                return Ok(ConnectionState {
                    state: ConnState::NotConfigured,
                    reason: Some("no refresh token".into()),
                });
            }
            let token = match self.access_token().await {
                Ok(token) => token,
                Err(error) => {
                    return Ok(ConnectionState {
                        state: ConnState::Failed,
                        reason: Some(Self::failed_reason(&error)),
                    })
                }
            };
            let response = match self.http.get(CHANNELS_URL).bearer_auth(&token).send().await {
                Ok(response) => response,
                Err(error) => {
                    return Ok(ConnectionState {
                        state: ConnState::Failed,
                        reason: Some(self.scrub(&format!("cannot reach YouTube: {error}"))),
                    })
                }
            };
            let status = response.status();
            let headers = response.headers().clone();
            let body = response.text().await.unwrap_or_default();
            if !status.is_success() {
                let error = self.map_status(status, &headers, &body);
                return Ok(ConnectionState {
                    state: ConnState::Failed,
                    reason: Some(Self::failed_reason(&error)),
                });
            }
            let value = match serde_json::from_str::<Value>(&body) {
                Ok(value) => value,
                Err(_) => {
                    return Ok(ConnectionState {
                        state: ConnState::Failed,
                        reason: Some("could not read the channel list".into()),
                    })
                }
            };
            let channel_id = value
                .get("items")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str);
            match channel_id {
                Some(id) => Ok(ConnectionState {
                    state: ConnState::Ready,
                    reason: Some(format!("channel {id}")),
                }),
                None => Ok(ConnectionState {
                    state: ConnState::Failed,
                    reason: Some("no YouTube channel on this Google account".into()),
                }),
            }
        })
    }

    fn publish(&self, payload: PublishPayload) -> BoxFuture<'_, Result<PublishOk, AdapterError>> {
        Box::pin(async move {
            let (title, truncated_from) = truncate_text(&payload.text, TITLE_MAX_LENGTH);
            let media = match payload.media.as_ref() {
                Some(media) if matches!(media.kind, MediaKind::Video | MediaKind::Image) => media,
                _ => {
                    return Err(AdapterError {
                        kind: ErrorKind::ContentInvalid,
                        message: TEXT_ONLY_MESSAGE.into(),
                        retry_after_seconds: None,
                    })
                }
            };
            if payload.text.trim().is_empty() {
                return Err(AdapterError {
                    kind: ErrorKind::ContentInvalid,
                    message: "no text for the title".into(),
                    retry_after_seconds: None,
                });
            }
            if payload.dry_run {
                return Ok(PublishOk {
                    ok: true,
                    remote_id: Some("dry-run".into()),
                    remote_url: None,
                    truncated_from,
                });
            }
            let path = match media.local_path.as_deref() {
                Some(path) => path,
                None => {
                    return Err(AdapterError {
                        kind: ErrorKind::MediaUnreachable,
                        message: "no local file for the video".into(),
                        retry_after_seconds: None,
                    })
                }
            };
            let mut file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(_) => {
                    return Err(AdapterError {
                        kind: ErrorKind::MediaUnreachable,
                        message: format!("cannot open {path}"),
                        retry_after_seconds: None,
                    })
                }
            };
            let total = match file.metadata() {
                Ok(meta) => meta.len(),
                Err(_) => {
                    return Err(AdapterError {
                        kind: ErrorKind::MediaUnreachable,
                        message: format!("cannot read the size of {path}"),
                        retry_after_seconds: None,
                    })
                }
            };
            if total == 0 {
                return Err(AdapterError {
                    kind: ErrorKind::ContentInvalid,
                    message: "the video file is empty".into(),
                    retry_after_seconds: None,
                });
            }
            let token = self.access_token().await?;
            let mime = mime_for(path);

            // Step 1: open a resumable session and take its Location.
            let init_body = serde_json::json!({
                "snippet": {
                    "title": title,
                    "description": payload.text,
                    "categoryId": "22",
                },
                "status": { "privacyStatus": "private" },
            });
            let init = self
                .http
                .post(UPLOAD_INIT_URL)
                .bearer_auth(&token)
                .header(CONTENT_TYPE, "application/json")
                .header("X-Upload-Content-Type", mime)
                .header("X-Upload-Content-Length", total.to_string())
                .json(&init_body)
                .send()
                .await
                .map_err(|error| self.map_transport("upload init", error))?;
            let status = init.status();
            let headers = init.headers().clone();
            let body = init.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(self.map_status(status, &headers, &body));
            }
            let location = match headers.get(LOCATION).and_then(|value| value.to_str().ok()) {
                Some(location) => location.to_string(),
                None => return Err(self.unknown("upload init (no upload session URL)", &body)),
            };

            // Step 2: PUT the file in bounded chunks; 308 keeps the session alive.
            let mut offset: u64 = 0;
            let mut reported_end: Option<u64> = None;
            let mut buf = vec![0u8; CHUNK_SIZE];
            loop {
                let n = read_chunk(&mut file, &mut buf).map_err(|error| AdapterError {
                    kind: ErrorKind::MediaUnreachable,
                    message: format!("cannot read {path}: {error}"),
                    retry_after_seconds: None,
                })?;
                if n == 0 {
                    return Err(AdapterError {
                        kind: ErrorKind::ContentInvalid,
                        message: "the file ended before the whole video was sent".into(),
                        retry_after_seconds: None,
                    });
                }
                let content_range =
                    format!("bytes {}-{}/{}", offset, offset + n as u64 - 1, total);
                let response = self
                    .http
                    .put(&location)
                    .header(CONTENT_TYPE, mime)
                    .header(CONTENT_RANGE, &content_range)
                    .body(buf[..n].to_vec())
                    .send()
                    .await
                    .map_err(|error| self.map_transport("upload", error))?;
                let status = response.status();
                let headers = response.headers().clone();
                let body = response.text().await.unwrap_or_default();
                if status.as_u16() == 308 {
                    match parse_range(&headers) {
                        Some(end) => {
                            if reported_end == Some(end) {
                                return Err(AdapterError {
                                    kind: ErrorKind::Unknown,
                                    message: self
                                        .scrub(&format!("upload stalled at byte {end}")),
                                    retry_after_seconds: None,
                                });
                            }
                            reported_end = Some(end);
                            offset = end + 1;
                            file.seek(SeekFrom::Start(offset)).map_err(|error| {
                                AdapterError {
                                    kind: ErrorKind::MediaUnreachable,
                                    message: format!("cannot reposition {path}: {error}"),
                                    retry_after_seconds: None,
                                }
                            })?;
                        }
                        // No Range header: trust the chunk was received and move on.
                        None => offset += n as u64,
                    }
                    continue;
                }
                if !status.is_success() {
                    return Err(self.map_status(status, &headers, &body));
                }
                let value = serde_json::from_str::<Value>(&body).map_err(|_| {
                    self.unknown("upload complete (unreadable response)", &body)
                })?;
                let video_id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| self.unknown("upload complete (no video id)", &body))?;
                return Ok(PublishOk {
                    ok: true,
                    remote_id: Some(video_id.clone()),
                    remote_url: Some(format!("https://youtu.be/{video_id}")),
                    truncated_from,
                });
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::Media;

    fn adapter() -> YouTubeAdapter {
        YouTubeAdapter::new(
            "client-id".into(),
            "client-secret".into(),
            "refresh-token".into(),
        )
    }

    #[test]
    fn capabilities_match_the_contract() {
        assert_eq!(
            adapter().capabilities(),
            Capabilities {
                platform: "youtube".into(),
                max_text_length: 5000,
                media_kinds: vec![MediaKind::Video, MediaKind::Image],
                honors_retry_after: true,
                supports_edit: false,
                supports_delete: true,
                supports_metrics: true,
                needs_public_image_url: false,
            }
        );
    }

    #[tokio::test]
    async fn empty_credentials_are_not_configured() {
        let empty = YouTubeAdapter::new(String::new(), String::new(), String::new());
        let state = empty.connection_state().await.unwrap();
        assert_eq!(state.state, ConnState::NotConfigured);

        let half = YouTubeAdapter::new(String::new(), String::new(), "refresh-token".into());
        let state = half.connection_state().await.unwrap();
        assert_eq!(state.state, ConnState::NotConfigured);
        assert_eq!(state.reason.as_deref(), Some("no client id"));

        let other = YouTubeAdapter::new("client-id".into(), String::new(), String::new());
        let state = other.connection_state().await.unwrap();
        assert_eq!(state.state, ConnState::NotConfigured);
        assert_eq!(state.reason.as_deref(), Some("no refresh token"));
    }

    #[tokio::test]
    async fn text_only_publish_is_refused_with_the_exact_message() {
        let error = adapter()
            .publish(PublishPayload::default())
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::ContentInvalid);
        assert_eq!(error.message, TEXT_ONLY_MESSAGE);
    }

    #[tokio::test]
    async fn publish_without_a_local_file_is_media_unreachable() {
        let payload = PublishPayload {
            text: "a title".into(),
            media: Some(Media {
                kind: MediaKind::Video,
                ..Default::default()
            }),
            ..Default::default()
        };
        let error = adapter().publish(payload).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::MediaUnreachable);
    }

    #[tokio::test]
    async fn dry_run_reports_the_title_truncation() {
        let text = "word ".repeat(30);
        let payload = PublishPayload {
            text: text.clone(),
            media: Some(Media {
                kind: MediaKind::Video,
                ..Default::default()
            }),
            dry_run: true,
            ..Default::default()
        };
        let ok = adapter().publish(payload).await.unwrap();
        assert!(ok.ok);
        assert_eq!(ok.truncated_from, Some(text.chars().count()));
    }

    #[test]
    fn maps_statuses_retry_after_and_transport_errors() {
        let adapter = adapter();
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, "12".parse().unwrap());

        let limited = adapter.map_status(
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            "client-secret and refresh-token leaked",
        );
        assert_eq!(limited.kind, ErrorKind::RateLimited);
        assert_eq!(limited.retry_after_seconds, Some(12));
        assert!(!limited.message.contains("client-secret"));
        assert!(!limited.message.contains("refresh-token"));
        assert!(limited.message.contains("[redacted]"));

        assert_eq!(
            adapter
                .map_status(StatusCode::UNAUTHORIZED, &headers, "{}")
                .kind,
            ErrorKind::AuthInvalid
        );
        assert_eq!(
            adapter
                .map_status(StatusCode::FORBIDDEN, &headers, "{}")
                .kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            adapter
                .map_status(StatusCode::BAD_REQUEST, &headers, "{}")
                .kind,
            ErrorKind::ContentInvalid
        );
        assert_eq!(
            adapter.map_status(StatusCode::UNPROCESSABLE_ENTITY, &headers, "{}").kind,
            ErrorKind::ContentInvalid
        );
        assert_eq!(
            adapter
                .map_status(StatusCode::SERVICE_UNAVAILABLE, &headers, "{}")
                .kind,
            ErrorKind::PlatformOutage
        );

        let network = adapter.map_transport("token refresh", "reset by refresh-token");
        assert_eq!(network.kind, ErrorKind::Network);
        assert!(!network.message.contains("refresh-token"));
        assert!(network.message.contains("reset by [redacted]"));
    }

    #[test]
    fn title_truncates_at_a_word_boundary() {
        let long = "alpha beta gamma delta ".repeat(10);
        let (title, original) = truncate_text(long.trim_end(), TITLE_MAX_LENGTH);
        assert!(title.chars().count() <= TITLE_MAX_LENGTH);
        assert!(title.ends_with('…'));
        assert_eq!(original, Some(long.trim_end().chars().count()));

        let (same, none) = truncate_text("a short title", TITLE_MAX_LENGTH);
        assert_eq!(same, "a short title");
        assert_eq!(none, None);
    }

    #[test]
    fn parse_range_reads_the_server_offset() {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RANGE,
            "bytes=0-8388607".parse().unwrap(),
        );
        assert_eq!(parse_range(&headers), Some(8_388_607));
        assert_eq!(parse_range(&HeaderMap::new()), None);
    }

    #[test]
    fn debug_hides_secrets() {
        let debug = format!("{:?}", adapter());
        assert!(!debug.contains("client-secret"));
        assert!(!debug.contains("refresh-token"));
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn form_values_are_encoded() {
        assert_eq!(form_value("a b&c"), "a%20b%26c");
    }
}
