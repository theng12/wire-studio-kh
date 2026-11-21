//! Facebook Page adapter skeleton. Posting is refused until 0.3.0; no token is logged.

use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;

use super::{
    AdapterError, BoxFuture, Capabilities, ConnState, ConnectionState, ErrorKind, MediaKind,
    PlatformAdapter, PublishOk, PublishPayload,
};

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
        serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(serde_json::Value::as_str)
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
            Ok(ConnectionState {
                state: ConnState::Failed,
                reason: Some("Page connection is checked when posting arrives in 0.3.0".into()),
            })
        })
    }

    fn publish(&self, _payload: PublishPayload) -> BoxFuture<'_, Result<PublishOk, AdapterError>> {
        Box::pin(async move {
            Err(AdapterError {
                kind: ErrorKind::Unknown,
                message: "Facebook Page posting arrives in 0.3.0".into(),
                retry_after_seconds: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let state = adapter().connection_state().await.unwrap();
        assert_eq!(state.state, ConnState::Failed);
        assert_eq!(
            state.reason.as_deref(),
            Some("Page connection is checked when posting arrives in 0.3.0")
        );
    }

    #[tokio::test]
    async fn publish_refuses_instead_of_pretending() {
        let error = adapter()
            .publish(PublishPayload::default())
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unknown);
        assert_eq!(error.message, "Facebook Page posting arrives in 0.3.0");
    }

    #[test]
    fn maps_statuses_retry_after_and_transport_errors() {
        let adapter = adapter();
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, "12".parse().unwrap());

        let limited = adapter.map_status(
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            "page-token leaked",
        );
        assert_eq!(limited.kind, ErrorKind::RateLimited);
        assert_eq!(limited.retry_after_seconds, Some(12));
        assert!(!limited.message.contains("page-token"));
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

        let network = adapter.map_transport("post", "reset by page-token");
        assert_eq!(network.kind, ErrorKind::Network);
        assert!(!network.message.contains("page-token"));
        assert!(network.message.contains("reset by [redacted]"));
    }

    #[test]
    fn debug_hides_the_token() {
        let debug = format!("{:?}", adapter());
        assert!(!debug.contains("page-token"));
        assert!(debug.contains("[redacted]"));
    }
}
