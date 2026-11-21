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

/// A verified pair: the bot the token belongs to, and the channel it answered for.
#[derive(Debug, Clone, PartialEq)]
pub struct TelegramCheck {
    pub bot: String,
    pub chat: String,
}

/// A channel the bot can see, in the words Telegram gave for it.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredChat {
    pub id: String,
    pub title: String,
    pub kind: String,
}

const BLOCKING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const INVITE_REASON: &str = "Invite links only work once the bot is in the channel — add the bot, post a message, then press Find my channel.";

/// A bounded, blocking check for the headless route (the windowed app awaits `check`).
pub fn check_blocking(token: &str, chat_id: &str) -> Result<TelegramCheck, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let adapter = TelegramAdapter::new(token.to_string());
    runtime
        .block_on(async {
            tokio::time::timeout(BLOCKING_TIMEOUT, adapter.check(chat_id)).await
        })
        .map_err(|_| "Telegram did not answer within 10 seconds".to_string())?
        .map_err(|error| error.message)
}

/// Bounded headless forms of the connect flow's three reads.
pub fn connected_as_blocking(token: &str) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let adapter = TelegramAdapter::new(token.to_string());
    runtime
        .block_on(async { tokio::time::timeout(BLOCKING_TIMEOUT, adapter.connected_as()).await })
        .map_err(|_| "Telegram did not answer within 10 seconds".to_string())?
        .map_err(|error| error.message)
}

pub fn discover_blocking(token: &str) -> Result<Vec<DiscoveredChat>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let adapter = TelegramAdapter::new(token.to_string());
    runtime
        .block_on(async { tokio::time::timeout(BLOCKING_TIMEOUT, adapter.discover()).await })
        .map_err(|_| "Telegram did not answer within 10 seconds".to_string())?
        .map_err(|error| error.message)
}

pub fn resolve_blocking(token: &str, text: &str) -> Result<DiscoveredChat, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let adapter = TelegramAdapter::new(token.to_string());
    runtime
        .block_on(async { tokio::time::timeout(BLOCKING_TIMEOUT, adapter.resolve(text)).await })
        .map_err(|_| "Telegram did not answer within 10 seconds".to_string())?
        .map_err(|error| error.message)
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

    /// One GET against the Bot API; `ok:false` bodies become the provider's own words.
    async fn get(&self, method: &str, query: &[(&str, &str)]) -> Result<Value, AdapterError> {
        let response = self
            .http
            .get(self.endpoint(method))
            .query(query)
            .send()
            .await
            .map_err(|error| AdapterError {
                kind: ErrorKind::Network,
                message: self.scrub(&format!("{method}: {error}")),
                retry_after_seconds: None,
            })?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(self.http_error(status, &headers, &body));
        }
        let value: Value = serde_json::from_str(&body)
            .map_err(|_| Self::unknown(self.scrub(&format!("{method}: {}", Self::description(&body)))))?;
        if value.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(Self::unknown(self.scrub(&format!("{method}: {}", Self::description(&body)))));
        }
        Ok(value)
    }

    /// Who the token belongs to, without needing a channel.
    pub async fn connected_as(&self) -> Result<String, AdapterError> {
        let me = self.get("getMe", &[]).await?;
        let username = me
            .get("result")
            .and_then(|row| row.get("username"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Ok(format!("@{username}"))
    }

    /// What the connection really is: the bot behind the token, and the channel it can see.
    pub async fn check(&self, chat_id: &str) -> Result<TelegramCheck, AdapterError> {
        let bot = self.connected_as().await?;
        let chat = self.get("getChat", &[("chat_id", chat_id)]).await?;
        Ok(TelegramCheck {
            bot,
            chat: chat_title(chat.get("result"), chat_id),
        })
    }

    /// Channels and groups the bot can see. No updates is a normal, empty answer.
    pub async fn discover(&self) -> Result<Vec<DiscoveredChat>, AdapterError> {
        let updates = self
            .get(
                "getUpdates",
                &[
                    ("limit", "100"),
                    ("timeout", "0"),
                    ("allowed_updates", r#"["message","channel_post"]"#),
                ],
            )
            .await?;
        Ok(discovered_from_updates(updates.get("result")))
    }

    /// Turn a pasted name, t.me link or numeric id into Telegram's own channel details.
    pub async fn resolve(&self, raw: &str) -> Result<DiscoveredChat, AdapterError> {
        let reference = chat_reference(raw).map_err(|message| AdapterError {
            kind: ErrorKind::ContentInvalid,
            message,
            retry_after_seconds: None,
        })?;
        let answer = self.get("getChat", &[("chat_id", reference.as_str())]).await?;
        discovered_chat(answer.get("result")).ok_or_else(|| {
            Self::unknown(self.scrub("Telegram did not return the channel details."))
        })
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

/// A channel has `title`, a supergroup handle has `username`, a person has `first_name`.
fn chat_title(result: Option<&Value>, fallback: &str) -> String {
    result
        .and_then(|row| {
            row.get("title")
                .or_else(|| row.get("username"))
                .or_else(|| row.get("first_name"))
        })
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

/// Bot API needs @name or a numeric id, never a bare t.me link or an invite link.
fn chat_reference(raw: &str) -> Result<String, String> {
    let raw = raw.trim().trim_end_matches('/');
    let path = if let Some((_, rest)) = raw.split_once("://") {
        let (host, path) = rest.split_once('/').ok_or("Use a t.me channel link.")?;
        if !host.eq_ignore_ascii_case("t.me") && !host.eq_ignore_ascii_case("www.t.me") {
            return Err("Use a t.me channel link.".into());
        }
        path
    } else {
        raw.strip_prefix("t.me/").unwrap_or(raw)
    };
    let handle = path.split(['?', '#']).next().unwrap_or("").trim();
    if handle.is_empty() {
        return Err("Add a channel name, t.me link or Telegram id.".into());
    }
    if handle.starts_with('+') {
        return Err(INVITE_REASON.into());
    }
    if handle.contains('/') {
        return Err("Open the channel from Find my channel; a post link cannot be added.".into());
    }
    if handle.parse::<i64>().is_ok() {
        return Ok(handle.to_string());
    }
    Ok(format!("@{}", handle.strip_prefix('@').unwrap_or(handle)))
}

fn discovered_chat(chat: Option<&Value>) -> Option<DiscoveredChat> {
    let row = chat?;
    let id = row
        .get("id")
        .and_then(Value::as_i64)
        .map(|id| id.to_string())
        .or_else(|| row.get("id").and_then(Value::as_str).map(str::to_string))?;
    let title = chat_title(Some(row), &id);
    Some(DiscoveredChat {
        id,
        title,
        kind: row.get("type").and_then(Value::as_str).unwrap_or("").into(),
    })
}

fn discovered_from_updates(updates: Option<&Value>) -> Vec<DiscoveredChat> {
    let mut chats: Vec<DiscoveredChat> = Vec::new();
    for update in updates.and_then(Value::as_array).into_iter().flatten() {
        let chat = update
            .get("message")
            .or_else(|| update.get("channel_post"))
            .and_then(|row| row.get("chat"));
        if let Some(found) = discovered_chat(chat) {
            // getUpdates can repeat a chat; keep the first sighting and its order.
            if !chats.iter().any(|known| known.id == found.id) {
                chats.push(found);
            }
        }
    }
    chats
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
                    message: "Pick a channel first.".into(),
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
    fn chat_title_prefers_the_real_name() {
        let json = |raw: &str| serde_json::from_str::<Value>(raw).unwrap();
        assert_eq!(chat_title(Some(&json(r#"{"title":"Wire Smoke Test"}"#)), "-100"), "Wire Smoke Test");
        assert_eq!(chat_title(Some(&json(r#"{"username":"wire_smoke"}"#)), "-100"), "wire_smoke");
        assert_eq!(chat_title(Some(&json(r#"{"first_name":"Sokha"}"#)), "-100"), "Sokha");
        assert_eq!(chat_title(Some(&json(r#"{}"#)), "-100"), "-100");
        assert_eq!(chat_title(None, "@c"), "@c");
    }

    #[test]
    fn chat_reference_accepts_names_links_and_numeric_ids() {
        assert_eq!(chat_reference("@news"), Ok("@news".into()));
        assert_eq!(chat_reference(" https://t.me/news "), Ok("@news".into()));
        assert_eq!(chat_reference("t.me/news/"), Ok("@news".into()));
        assert_eq!(chat_reference("http://www.t.me/news"), Ok("@news".into()));
        assert_eq!(chat_reference("-1004483436341"), Ok("-1004483436341".into()));
        assert_eq!(chat_reference("42"), Ok("42".into()));
    }

    #[test]
    fn chat_reference_refuses_invites_and_post_links() {
        let invite = chat_reference("t.me/+abc").unwrap_err();
        assert_eq!(invite, INVITE_REASON);
        assert_eq!(chat_reference("https://t.me/+abc").unwrap_err(), INVITE_REASON);
        assert!(chat_reference("https://t.me/news/42").unwrap_err().contains("post link"));
        assert!(chat_reference("https://example.com/news").unwrap_err().contains("t.me"));
        assert!(chat_reference("").unwrap_err().contains("channel"));
    }

    #[test]
    fn discovered_chats_keep_first_sighting_and_telegram_words() {
        let updates: Value = serde_json::from_str(
            r#"[
                {"update_id":1,"message":{"chat":{"id":-1001,"type":"channel","title":"Wire News"}}},
                {"update_id":2,"channel_post":{"chat":{"id":-1001,"type":"channel","title":"Wire News"}}},
                {"update_id":3,"message":{"chat":{"id":42,"type":"private","first_name":"Sokha"}}},
                {"update_id":4,"message":{"text":"no chat here"}},
                {"update_id":5,"channel_post":{"chat":{"id":-1002,"type":"supergroup","username":"wire_group"}}}
            ]"#,
        )
        .unwrap();
        assert_eq!(
            discovered_from_updates(Some(&updates)),
            vec![
                DiscoveredChat { id: "-1001".into(), title: "Wire News".into(), kind: "channel".into() },
                DiscoveredChat { id: "42".into(), title: "Sokha".into(), kind: "private".into() },
                DiscoveredChat { id: "-1002".into(), title: "wire_group".into(), kind: "supergroup".into() },
            ]
        );
        assert!(discovered_from_updates(None).is_empty());
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
