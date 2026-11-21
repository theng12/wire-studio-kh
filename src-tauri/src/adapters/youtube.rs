//! YouTube adapter skeleton. Posting is refused until 0.3.0; no secret is logged.

use reqwest::header::{HeaderMap, CONTENT_TYPE, RETRY_AFTER};
use reqwest::StatusCode;
use serde_json::Value;

use super::{
    AdapterError, BoxFuture, Capabilities, ConnState, ConnectionState, ErrorKind, MediaKind,
    PlatformAdapter, PublishOk, PublishPayload,
};

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const MAX_TEXT_LENGTH: usize = 5000;

pub struct YouTubeAdapter {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub http: reqwest::Client,
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
            return Err(self.map_status(status, &headers, &body));
        }
        let value: Value = serde_json::from_str(&body)
            .map_err(|_| self.unknown("token refresh", &body))?;
        value
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| self.unknown("token refresh", &body))
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
            if self.refresh_token.trim().is_empty() {
                return Ok(ConnectionState {
                    state: ConnState::NotConfigured,
                    reason: Some("no refresh token".into()),
                });
            }
            Ok(ConnectionState {
                state: ConnState::Ready,
                reason: Some("not verified in 0.1.0".into()),
            })
        })
    }

    fn publish(&self, _payload: PublishPayload) -> BoxFuture<'_, Result<PublishOk, AdapterError>> {
        Box::pin(async move {
            Err(AdapterError {
                kind: ErrorKind::Unknown,
                message: "YouTube posting arrives in 0.3.0".into(),
                retry_after_seconds: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn missing_refresh_token_is_not_configured() {
        let empty = YouTubeAdapter::new(String::new(), String::new(), String::new());
        let state = empty.connection_state().await.unwrap();
        assert_eq!(state.state, ConnState::NotConfigured);

        let state = adapter().connection_state().await.unwrap();
        assert_eq!(state.state, ConnState::Ready);
        assert_eq!(state.reason.as_deref(), Some("not verified in 0.1.0"));
    }

    #[tokio::test]
    async fn publish_refuses_instead_of_pretending() {
        let error = adapter()
            .publish(PublishPayload::default())
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unknown);
        assert_eq!(error.message, "YouTube posting arrives in 0.3.0");
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
                .map_status(StatusCode::SERVICE_UNAVAILABLE, &headers, "{}")
                .kind,
            ErrorKind::PlatformOutage
        );
        assert_eq!(
            adapter
                .map_status(StatusCode::BAD_REQUEST, &headers, "{}")
                .kind,
            ErrorKind::Unknown
        );

        let network = adapter.map_transport("token refresh", "reset by refresh-token");
        assert_eq!(network.kind, ErrorKind::Network);
        assert!(!network.message.contains("refresh-token"));
        assert!(network.message.contains("reset by [redacted]"));
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
