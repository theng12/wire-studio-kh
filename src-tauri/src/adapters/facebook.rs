//! Facebook Page adapter (Graph API v21.0). The page token never reaches a log line or error text.

use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;
use serde_json::Value;

use super::{
    truncate_text, AdapterError, BoxFuture, Capabilities, ConnState, ConnectionState, ErrorKind,
    MediaKind, PlatformAdapter, PublishOk, PublishPayload,
};

const API_ROOT: &str = "https://graph.facebook.com/v21.0";
const MAX_TEXT_LENGTH: usize = 63206;

pub struct FacebookPageAdapter {
    pub page_id: String,
    pub page_token: String,
    pub http: reqwest::Client,
}

impl std::fmt::Debug for FacebookPageAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FacebookPageAdapter")
            .field("page_token", &"[redacted]")
            .finish_non_exhaustive()
    }
}

/// Pure request builder so tests can check endpoint and fields without network.
fn text_post_request(page_id: &str, text: &str, token: &str) -> (String, Vec<(String, String)>) {
    (
        format!("{API_ROOT}/{page_id}/feed"),
        vec![
            ("message".to_string(), text.to_string()),
            ("access_token".to_string(), token.to_string()),
        ],
    )
}

/// Pure request builder for a photo post served from a public URL.
fn image_post_request(
    page_id: &str,
    caption: &str,
    public_url: &str,
    token: &str,
) -> (String, Vec<(String, String)>) {
    (
        format!("{API_ROOT}/{page_id}/photos"),
        vec![
            ("caption".to_string(), caption.to_string()),
            ("url".to_string(), public_url.to_string()),
            ("access_token".to_string(), token.to_string()),
        ],
    )
}

impl FacebookPageAdapter {
    pub fn new(page_id: String, page_token: String) -> Self {
        Self {
            page_id,
            page_token,
            http: reqwest::Client::new(),
        }
    }

    /// reqwest errors can echo the request, which carries the page token.
    fn scrub(&self, text: &str) -> String {
        if self.page_token.is_empty() {
            text.to_string()
        } else {
            text.replace(&self.page_token, "[redacted]")
        }
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

    fn error_fields(body: &str) -> (Option<i64>, Option<String>) {
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| {
                let error = value.get("error")?;
                let code = error.get("code").and_then(Value::as_i64);
                let kind = error
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                Some((code, kind))
            })
            .unwrap_or((None, None))
    }

    /// Graph reports conditions in both the HTTP status and the error code; the
    /// code wins when both are present (rate limits often ride a 400).
    fn map_status(&self, status: StatusCode, headers: &HeaderMap, body: &str) -> AdapterError {
        let (code, error_type) = Self::error_fields(body);
        let kind = if status == StatusCode::UNAUTHORIZED || error_type.as_deref() == Some("OAuthException") {
            ErrorKind::AuthInvalid
        } else if status == StatusCode::FORBIDDEN || code == Some(200) || code == Some(10) {
            ErrorKind::PermissionDenied
        } else if status == StatusCode::TOO_MANY_REQUESTS
            || matches!(code, Some(4) | Some(17) | Some(32) | Some(613))
        {
            ErrorKind::RateLimited
        } else if status == StatusCode::BAD_REQUEST && matches!(code, Some(100) | Some(190)) {
            ErrorKind::ContentInvalid
        } else if status.is_server_error() {
            ErrorKind::PlatformOutage
        } else {
            ErrorKind::Unknown
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
            message: self.scrub(&format!("Facebook HTTP {status}: {}", Self::description(body))),
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

    /// shortcut: access_token rides the query string unencoded; page tokens are
    /// alphanumeric so this is safe today — encode if tokens ever change shape.
    fn page_endpoint(&self, query: &str) -> String {
        format!("{API_ROOT}/{}?{query}&access_token={}", self.page_id, self.page_token)
    }
}

impl PlatformAdapter for FacebookPageAdapter {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: "facebook_page".into(),
            max_text_length: MAX_TEXT_LENGTH,
            media_kinds: vec![MediaKind::Image, MediaKind::Video],
            honors_retry_after: true,
            supports_edit: true,
            supports_delete: true,
            supports_metrics: true,
            needs_public_image_url: false,
        }
    }

    fn connection_state(&self) -> BoxFuture<'_, Result<ConnectionState, AdapterError>> {
        Box::pin(async move {
            if self.page_id.trim().is_empty() || self.page_token.trim().is_empty() {
                return Ok(ConnectionState {
                    state: ConnState::NotConfigured,
                    reason: Some("no page id or page token".into()),
                });
            }
            let response = self
                .http
                .get(self.page_endpoint("fields=id,name"))
                .send()
                .await
                .map_err(|error| {
                    AdapterError {
                        kind: ErrorKind::Network,
                        message: self.scrub(&format!("page lookup: {error}")),
                        retry_after_seconds: None,
                    }
                })?;
            let status = response.status();
            let headers = response.headers().clone();
            let body = response.text().await.unwrap_or_default();
            if !status.is_success() {
                let error = self.map_status(status, &headers, &body);
                return Ok(ConnectionState {
                    state: ConnState::Failed,
                    reason: Some(error.message),
                });
            }
            let value: Value = serde_json::from_str(&body).map_err(|_| AdapterError {
                kind: ErrorKind::Unknown,
                message: self.scrub(&format!("page lookup: {}", Self::description(&body))),
                retry_after_seconds: None,
            })?;
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Ok(ConnectionState {
                state: ConnState::Ready,
                reason: Some(format!("Page {name}")),
            })
        })
    }

    fn publish(&self, payload: PublishPayload) -> BoxFuture<'_, Result<PublishOk, AdapterError>> {
        Box::pin(async move {
            let (text, truncated_from) =
                truncate_text(&payload.text, self.capabilities().max_text_length);
            if payload.dry_run {
                return Ok(PublishOk {
                    ok: true,
                    remote_id: Some("dry-run".into()),
                    remote_url: None,
                    truncated_from,
                });
            }
            if payload.target.trim().is_empty() {
                return Err(AdapterError {
                    kind: ErrorKind::ContentInvalid,
                    message: "no page id target".into(),
                    retry_after_seconds: None,
                });
            }
            let (endpoint, fields) = match payload.media {
                None => text_post_request(&payload.target, &text, &self.page_token),
                Some(media) if media.kind == MediaKind::Image => {
                    let public_url = media.public_url.ok_or_else(|| AdapterError {
                        kind: ErrorKind::MediaUnreachable,
                        message: "Facebook needs a public image address for this post".into(),
                        retry_after_seconds: None,
                    })?;
                    image_post_request(&payload.target, &text, &public_url, &self.page_token)
                }
                Some(_) => {
                    return Err(AdapterError {
                        kind: ErrorKind::Unknown,
                        message: "Facebook video posting is not implemented yet".into(),
                        retry_after_seconds: None,
                    })
                }
            };
            let response = self
                .http
                .post(&endpoint)
                .form(&fields)
                .send()
                .await
                .map_err(|error| self.map_transport("post", error))?;
            let status = response.status();
            let headers = response.headers().clone();
            let response_body = response.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(self.map_status(status, &headers, &response_body));
            }
            let value: Value = serde_json::from_str(&response_body).map_err(|_| AdapterError {
                kind: ErrorKind::Unknown,
                message: self.scrub(&format!("post: {}", Self::description(&response_body))),
                retry_after_seconds: None,
            })?;
            let id = value.get("id").and_then(Value::as_str).ok_or_else(|| {
                AdapterError {
                    kind: ErrorKind::Unknown,
                    message: self.scrub(&format!("post: {}", Self::description(&response_body))),
                    retry_after_seconds: None,
                }
            })?;
            Ok(PublishOk {
                ok: true,
                remote_id: Some(id.to_string()),
                remote_url: Some(format!("https://www.facebook.com/{id}")),
                truncated_from,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::Media;

    fn adapter() -> FacebookPageAdapter {
        FacebookPageAdapter::new("page-42".into(), "page-token".into())
    }

    #[test]
    fn capabilities_match_the_contract() {
        assert_eq!(
            adapter().capabilities(),
            Capabilities {
                platform: "facebook_page".into(),
                max_text_length: 63206,
                media_kinds: vec![MediaKind::Image, MediaKind::Video],
                honors_retry_after: true,
                supports_edit: true,
                supports_delete: true,
                supports_metrics: true,
                needs_public_image_url: false,
            }
        );
    }

    #[tokio::test]
    async fn missing_credentials_are_not_configured() {
        for empty in [
            FacebookPageAdapter::new(String::new(), "page-token".into()),
            FacebookPageAdapter::new("page-42".into(), String::new()),
        ] {
            let state = empty.connection_state().await.unwrap();
            assert_eq!(state.state, ConnState::NotConfigured);
        }
    }

    #[test]
    fn text_post_builds_the_feed_endpoint_and_fields() {
        let (endpoint, fields) = text_post_request("page-42", "hello", "page-token");
        assert_eq!(endpoint, "https://graph.facebook.com/v21.0/page-42/feed");
        assert_eq!(
            fields,
            vec![
                ("message".to_string(), "hello".to_string()),
                ("access_token".to_string(), "page-token".to_string()),
            ]
        );
    }

    #[test]
    fn image_post_builds_the_photos_endpoint_and_fields() {
        let (endpoint, fields) = image_post_request("page-42", "cap", "https://cdn/x.png", "page-token");
        assert_eq!(endpoint, "https://graph.facebook.com/v21.0/page-42/photos");
        assert_eq!(
            fields,
            vec![
                ("caption".to_string(), "cap".to_string()),
                ("url".to_string(), "https://cdn/x.png".to_string()),
                ("access_token".to_string(), "page-token".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn image_without_public_url_is_refused() {
        let payload = PublishPayload {
            target: "page-42".into(),
            text: "caption".into(),
            media: Some(Media {
                kind: MediaKind::Image,
                public_url: None,
                local_path: Some("/tmp/local.png".into()),
            }),
            dry_run: false,
        };
        let error = adapter().publish(payload).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::MediaUnreachable);
        assert_eq!(
            error.message,
            "Facebook needs a public image address for this post"
        );
    }

    #[test]
    fn maps_statuses_and_error_codes() {
        let adapter = adapter();
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, "12".parse().unwrap());

        let cases: &[(StatusCode, &str, ErrorKind)] = &[
            (StatusCode::UNAUTHORIZED, "{}", ErrorKind::AuthInvalid),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"bad oauth","type":"OAuthException","code":400}}"#,
                ErrorKind::AuthInvalid,
            ),
            (StatusCode::FORBIDDEN, "{}", ErrorKind::PermissionDenied),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"no admin","code":200}}"#,
                ErrorKind::PermissionDenied,
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"app 10","code":10}}"#,
                ErrorKind::PermissionDenied,
            ),
            (StatusCode::TOO_MANY_REQUESTS, "{}", ErrorKind::RateLimited),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"too many calls","code":4}}"#,
                ErrorKind::RateLimited,
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"app limit","code":17}}"#,
                ErrorKind::RateLimited,
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"thread limit","code":32}}"#,
                ErrorKind::RateLimited,
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"user limit","code":613}}"#,
                ErrorKind::RateLimited,
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"invalid param","code":100}}"#,
                ErrorKind::ContentInvalid,
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"token expired","code":190}}"#,
                ErrorKind::ContentInvalid,
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "{}",
                ErrorKind::PlatformOutage,
            ),
            (
                StatusCode::NOT_FOUND,
                "{}",
                ErrorKind::Unknown,
            ),
        ];
        for (status, body, kind) in cases {
            assert_eq!(adapter.map_status(*status, &headers, body).kind, *kind);
        }

        let limited = adapter.map_status(
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            r#"{"error":{"message":"page-token leaked","code":4}}"#,
        );
        assert_eq!(limited.kind, ErrorKind::RateLimited);
        assert_eq!(limited.retry_after_seconds, Some(12));
        assert!(limited.message.contains("too") || limited.message.contains("leaked"));
        assert!(!limited.message.contains("page-token"));
        assert!(limited.message.contains("[redacted]"));

        // Rate limit without a Retry-After header reports none, not zero.
        assert_eq!(
            adapter
                .map_status(StatusCode::TOO_MANY_REQUESTS, &HeaderMap::new(), "{}")
                .retry_after_seconds,
            None
        );

        let network = adapter.map_transport("post", "reset by page-token");
        assert_eq!(network.kind, ErrorKind::Network);
        assert!(!network.message.contains("page-token"));
        assert!(network.message.contains("reset by [redacted]"));
    }

    #[test]
    fn graph_error_message_is_read_into_the_plain_message() {
        let adapter = adapter();
        let error = adapter.map_status(
            StatusCode::BAD_REQUEST,
            &HeaderMap::new(),
            r#"{"error":{"message":"The message cannot be empty","code":100}}"#,
        );
        assert!(error.message.contains("The message cannot be empty"));
    }

    #[test]
    fn debug_hides_the_token() {
        let debug = format!("{:?}", adapter());
        assert!(!debug.contains("page-token"));
        assert!(debug.contains("[redacted]"));
    }
}
