//! Read-only Gateway client. Every call reads live; nothing is cached.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Anything Gateway itself can answer is bounded by one number, like every other read here.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// What `/v1/models` published: the rows, and whether Gateway says that read is stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalogue {
    pub models: Vec<ModelInfo>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context: Option<i64>,
}

pub struct GatewayClient {
    pub base_url: String,
    pub token: Option<String>,
}

impl std::fmt::Debug for GatewayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayClient")
            .field("base_url", &self.base_url)
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl GatewayClient {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token,
        }
    }

    fn scrub(&self, text: &str) -> String {
        match self.token.as_deref() {
            Some(token) if !token.is_empty() => text.replace(token, "[redacted]"),
            _ => text.to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    async fn get(&self, path: &str) -> Result<String, String> {
        let client = reqwest::Client::new();
        let mut request = client.get(self.url(path));
        if let Some(token) = self.token.as_deref() {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|error| {
            let base = self.base_url.trim_end_matches('/');
            let text = cause(&error);
            // The plainest true sentence beats reqwest's URL-and-errno chain.
            let plain = if text.contains("Connection refused") {
                format!("Nothing is listening at {base}.")
            } else if error.is_timeout() {
                format!("{base} did not answer in time.")
            } else {
                format!("Could not reach {base}: {text}")
            };
            self.scrub(&plain)
        })?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(self.scrub(&sentence(status.as_u16(), &body)));
        }
        Ok(self.scrub(&body))
    }

    /// Live health answer from `/health`, trimmed for display.
    pub async fn health(&self) -> Result<String, String> {
        Ok(short(&self.get("/health").await?))
    }

    /// Live model catalogue from `/v1/models` — the only route that answers a key's fate.
    /// `/health` is public and says 200 to any bearer, so it is never the connection proof.
    pub async fn catalogue(&self) -> Result<Catalogue, String> {
        let body = self.get("/v1/models").await?;
        parse_models(&body)
    }
}

/// reqwest's own message stops at the URL; the reason ("Connection refused") lives in its source.
fn cause(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        text.push_str(": ");
        text.push_str(&inner.to_string());
        source = inner.source();
    }
    text
}

/// Gateway's own refusal sentence, without the JSON around it (`Invalid API key.`).
fn sentence(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("detail")
                .or_else(|| value.get("error").and_then(|e| e.get("message")))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    match detail {
        Some(text) if !text.trim().is_empty() => text.trim().to_string(),
        _ => format!("Gateway answered HTTP {status}: {}", short(body)),
    }
}

/// One connect attempt, shared by the window and the headless route so they cannot drift.
/// An empty url or key keeps what is saved; both empty is "Check connection".
pub async fn connect(
    data_dir: &std::path::Path,
    url: &str,
    key: &str,
) -> serde_json::Value {
    let secrets_path = crate::secrets::path_in(data_dir);
    let mut sec = crate::secrets::load(&secrets_path);
    if !key.trim().is_empty() {
        sec.gateway_key = key.trim().to_string();
    }
    let settings_path = crate::settings::path_in(data_dir);
    let mut cfg = crate::settings::load(&settings_path);
    if !url.trim().is_empty() {
        cfg.gateway_url = url.trim().trim_end_matches('/').to_string();
    }
    if cfg.gateway_url.is_empty() {
        return serde_json::json!({ "ok": false, "reason": "Add the Gateway address first." });
    }
    if sec.gateway_key.is_empty() {
        return serde_json::json!({ "ok": false, "reason": "Add the Gateway API key first." });
    }
    if let Err(error) = crate::secrets::save(&secrets_path, &sec) {
        return serde_json::json!({ "ok": false, "error": format!("cannot save the Gateway key: {error}") });
    }

    // Saved proves nothing: ask Gateway now, with the same bounded read the app makes.
    let client = GatewayClient::new(cfg.gateway_url.clone(), Some(sec.gateway_key.clone()));
    let answer = tokio::time::timeout(CONNECT_TIMEOUT, client.catalogue()).await;
    let (state, reason) = match answer {
        Ok(Ok(catalogue)) => {
            cfg.gateway_verified = Some(crate::settings::GatewayCheck {
                models: catalogue.models.len(),
                stale: catalogue.stale,
                at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
            cfg.gateway_verify_error.clear();
            crate::settings::gateway_status(true, &cfg.gateway_url, cfg.gateway_verified.as_ref(), "")
        }
        Ok(Err(error)) => {
            cfg.gateway_verified = None;
            cfg.gateway_verify_error = error;
            ("failed".to_string(), cfg.gateway_verify_error.clone())
        }
        Err(_) => {
            cfg.gateway_verified = None;
            cfg.gateway_verify_error = format!("Gateway did not answer within 10 seconds.");
            ("failed".to_string(), cfg.gateway_verify_error.clone())
        }
    };
    if let Err(error) = crate::settings::save(&settings_path, &cfg) {
        return serde_json::json!({ "ok": false, "error": format!("cannot save Gateway settings: {error}") });
    }
    serde_json::json!({
        "ok": state == "ready",
        "connection_state": state,
        "reason": reason,
        "url": cfg.gateway_url,
        "models": cfg.gateway_verified.as_ref().map(|check| check.models),
        "key_masked": crate::secrets::mask(&sec.gateway_key),
    })
}

/// The one block both state doorways publish, so the window and the headless route cannot drift.
pub fn status_json(data_dir: &std::path::Path) -> Value {
    let cfg = crate::settings::load(&crate::settings::path_in(data_dir));
    let sec = crate::secrets::load(&crate::secrets::path_in(data_dir));
    let has_key = !sec.gateway_key.is_empty();
    let (connection, reason) = crate::settings::gateway_status(
        has_key,
        &cfg.gateway_url,
        cfg.gateway_verified.as_ref(),
        &cfg.gateway_verify_error,
    );
    serde_json::json!({
        "configured": has_key && !cfg.gateway_url.is_empty(),
        "url": cfg.gateway_url,
        "key_masked": if has_key { crate::secrets::mask(&sec.gateway_key) } else { String::new() },
        "models": cfg.gateway_verified.as_ref().map(|check| check.models),
        "connection_state": connection,
        "reason": reason,
    })
}

/// The headless twin, for the test-mode route that has no tokio runtime of its own.
pub fn connect_blocking(data_dir: &std::path::Path, url: &str, key: &str) -> serde_json::Value {
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(connect(data_dir, url, key)),
        Err(error) => serde_json::json!({ "ok": false, "error": error.to_string() }),
    }
}

fn short(body: &str) -> String {
    body.trim().chars().take(300).collect()
}

fn parse_models(body: &str) -> Result<Catalogue, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model catalogue: {error}"))?;
    let rows = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or("model catalogue has no models array")?;
    let stale = value
        .get("freshness")
        .and_then(|freshness| freshness.get("stale"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let models = rows
        .iter()
        .map(|row| {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .ok_or("model row has no id")?
                .to_string();
            let name = row
                .get("display_name")
                .and_then(Value::as_str)
                .or_else(|| row.get("name").and_then(Value::as_str))
                .unwrap_or(&id)
                .to_string();
            let context = row
                .get("context_window_tokens")
                .or_else(|| row.get("context_length"))
                .and_then(Value::as_i64);
            Ok(ModelInfo { id, name, context })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Catalogue { models, stale })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gateway_catalogue_rows() {
        let body = r#"{"models":[
            {"id":"local::gemma","display_name":"Gemma","context_window_tokens":262144},
            {"id":"openai::gpt","name":"GPT"}
        ],"freshness":{"stale":true}}"#;
        assert_eq!(
            parse_models(body).unwrap(),
            Catalogue {
                stale: true,
                models: vec![
                    ModelInfo {
                        id: "local::gemma".into(),
                        name: "Gemma".into(),
                        context: Some(262144),
                    },
                    ModelInfo {
                        id: "openai::gpt".into(),
                        name: "GPT".into(),
                        context: None,
                    },
                ],
            }
        );
        // An empty catalogue is a healthy answer, not a parse failure.
        assert_eq!(
            parse_models(r#"{"models":[],"freshness":{"stale":false}}"#).unwrap(),
            Catalogue { models: vec![], stale: false }
        );
    }

    #[test]
    fn refusals_keep_gateways_own_sentence() {
        assert_eq!(
            sentence(403, r#"{"detail":"Invalid API key.","code":"FORBIDDEN"}"#),
            "Invalid API key."
        );
        assert_eq!(sentence(401, r#"{"detail":"Missing bearer token."}"#), "Missing bearer token.");
        assert_eq!(sentence(502, "Bad Gateway"), "Gateway answered HTTP 502: Bad Gateway");
    }

    #[test]
    fn nothing_listening_says_so_in_plain_words() {
        // A port that was just free and is now closed: refusal is deterministic, not lucky.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let client = GatewayClient::new(format!("http://127.0.0.1:{port}"), Some("secret-key-value".into()));
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(client.catalogue())
            .unwrap_err();
        assert!(error.contains(&format!("Nothing is listening at http://127.0.0.1:{port}.")), "{error}");
        assert!(!error.contains("secret-key-value"), "the key must never reach the sentence: {error}");
    }
}
