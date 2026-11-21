//! Telegram Bot API adapter. The bot token never reaches a log line or error text.

use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;
use serde_json::Value;

use super::{
    truncate_text, AdapterError, BoxFuture, Capabilities, ConnState, ConnectionState, ErrorKind,
    MediaKind, PlatformAdapter, PublishOk, PublishPayload,
};

const API_ROOT: &str = "https://api.telegram.org";
const MAX_TEXT_LENGTH: usize = 4096;

pub struct TelegramAdapter {
    pub token: String,
    pub http: reqwest::Client,
}

impl std::fmt::Debug for TelegramAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramAdapter")
            .field("token", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl TelegramAdapter {
    pub fn new(token: String) -> Self {
        Self {
            token,
            http: reqwest::Client::new(),
        }
    }

    fn endpoint(&self, method: &str) -> String {
        format!("{API_ROOT}/bot{}/{method}", self.token)
    }

    /// reqwest errors can echo the request URL, which carries the bot token.
    fn scrub(&self, text: &str) -> String {
        if self.token.is_empty() {
            text.to_string()
        } else {
            text.replace(&self.token, "[redacted]")
        }
    }

    fn description(body: &str) -> String {
        let fallback: String = body.chars().take(200).collect();
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("description")
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

    fn body_retry_after(body: &str) -> Option<u64> {
        let value: Value = serde_json::from_str(body).ok()?;
        let raw = value.get("parameters")?.get("retry_after")?;
        raw.as_u64()
            .or_else(|| raw.as_str()?.trim().parse::<u64>().ok())
    }

    fn http_error(&self, status: StatusCode, headers: &HeaderMap, body: &str) -> AdapterError {
        let kind = match status.as_u16() {
            401 => ErrorKind::AuthInvalid,
            403 => ErrorKind::PermissionDenied,
            429 => ErrorKind::RateLimited,
            400 => ErrorKind::ContentInvalid,
            500..=599 => ErrorKind::PlatformOutage,
            _ => ErrorKind::Unknown,
        };
        let mut message = format!("Telegram HTTP {}: {}", status, Self::description(body));
        if status == StatusCode::UNAUTHORIZED {
            message.push_str(" (check the bot token)");
        }
        let retry_after_seconds = if kind == ErrorKind::RateLimited {
            headers
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
                .or_else(|| Self::body_retry_after(body))
        } else {
            None
        };
        AdapterError {
            kind,
            message: self.scrub(&message),
            retry_after_seconds,
        }
    }

    fn unknown(message: String) -> AdapterError {
        AdapterError {
            kind: ErrorKind::Unknown,
            message,
            retry_after_seconds: None,
        }
    }

    /// Best-effort public link: only public channels and supergroups have one.
    fn message_url(chat_id: &str, message_id: i64) -> Option<String> {
        if let Some(handle) = chat_id.strip_prefix('@') {
            return Some(format!("https://t.me/{handle}/{message_id}"));
        }
        chat_id
            .strip_prefix("-100")
            .map(|id| format!("https://t.me/c/{id}/{message_id}"))
    }
}

impl PlatformAdapter for TelegramAdapter {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: "telegram".into(),
            max_text_length: MAX_TEXT_LENGTH,
            media_kinds: vec![MediaKind::None],
            honors_retry_after: true,
            supports_edit: false,
            supports_delete: false,
            supports_metrics: false,
            needs_public_image_url: false,
        }
    }

    fn connection_state(&self) -> BoxFuture<'_, Result<ConnectionState, AdapterError>> {
        Box::pin(async move {
            if self.token.trim().is_empty() {
                return Ok(ConnectionState {
                    state: ConnState::NotConfigured,
                    reason: Some("no bot token".into()),
                });
            }
            let response = self
                .http
                .get(self.endpoint("getMe"))
                .send()
                .await
                .map_err(|error| {
                    AdapterError {
                        kind: ErrorKind::Network,
                        message: self.scrub(&format!("getMe: {error}")),
                        retry_after_seconds: None,
                    }
                })?;
            let status = response.status();
            let headers = response.headers().clone();
            let body = response.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(self.http_error(status, &headers, &body));
            }
            let value: Value = serde_json::from_str(&body)
                .map_err(|_| Self::unknown(self.scrub(&format!("getMe: {}", Self::description(&body)))))?;
            if value.get("ok").and_then(Value::as_bool) != Some(true) {
                return Err(Self::unknown(self.scrub(&format!(
                    "getMe: {}",
                    Self::description(&body)
                ))));
            }
            let me = value.get("result");
            let username = me
                .and_then(|row| row.get("username"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let id = me
                .and_then(|row| row.get("id"))
                .and_then(Value::as_i64)
                .map(|id| format!(" (id {id})"))
                .unwrap_or_default();
            Ok(ConnectionState {
                state: ConnState::Ready,
                reason: Some(format!("bot @{username}{id}")),
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
                    message: "no chat_id target".into(),
                    retry_after_seconds: None,
                });
            }
            let body = serde_json::json!({
                "chat_id": &payload.target,
                "text": text,
                "disable_notification": false,
            });
            let response = self
                .http
                .post(self.endpoint("sendMessage"))
                .json(&body)
                .send()
                .await
                .map_err(|error| AdapterError {
                    kind: ErrorKind::Network,
                    message: self.scrub(&format!("sendMessage: {error}")),
                    retry_after_seconds: None,
                })?;
            let status = response.status();
            let headers = response.headers().clone();
            let response_body = response.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(self.http_error(status, &headers, &response_body));
            }
            let value: Result<Value, AdapterError> = serde_json::from_str(&response_body)
                .map_err(|_| Self::unknown(self.scrub(&format!(
                    "sendMessage: {}",
                    Self::description(&response_body)
                ))));
            let value = value?;
            if value.get("ok").and_then(Value::as_bool) != Some(true) {
                return Err(Self::unknown(self.scrub(&format!(
                    "sendMessage: {}",
                    Self::description(&response_body)
                ))));
            }
            let message_id = value
                .get("result")
                .and_then(|result| result.get("message_id"))
                .and_then(Value::as_i64);
            Ok(PublishOk {
                ok: true,
                remote_id: message_id.map(|id| id.to_string()),
                remote_url: message_id.and_then(|id| Self::message_url(&payload.target, id)),
                truncated_from,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_telegram_statuses_and_retry_after() {
        let adapter = TelegramAdapter::new("123:secret-token".into());
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, "12".parse().unwrap());

        let limited = adapter.http_error(StatusCode::TOO_MANY_REQUESTS, &headers, "{}");
        assert_eq!(limited.kind, ErrorKind::RateLimited);
        assert_eq!(limited.retry_after_seconds, Some(12));
        assert!(!limited.message.contains("secret-token"));

        assert_eq!(
            adapter
                .http_error(StatusCode::UNAUTHORIZED, &headers, "{}")
                .kind,
            ErrorKind::AuthInvalid
        );
        assert_eq!(
            adapter
                .http_error(StatusCode::FORBIDDEN, &headers, "{}")
                .kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            adapter
                .http_error(StatusCode::BAD_REQUEST, &headers, "{}")
                .kind,
            ErrorKind::ContentInvalid
        );
        assert_eq!(
            adapter
                .http_error(StatusCode::SERVICE_UNAVAILABLE, &headers, "{}")
                .kind,
            ErrorKind::PlatformOutage
        );
    }

    #[test]
    fn public_links_only_for_public_channels() {
        assert_eq!(
            TelegramAdapter::message_url("@news", 42).as_deref(),
            Some("https://t.me/news/42")
        );
        assert_eq!(
            TelegramAdapter::message_url("-1001234", 42).as_deref(),
            Some("https://t.me/c/1234/42")
        );
        assert_eq!(TelegramAdapter::message_url("12345", 42), None);
    }
}
