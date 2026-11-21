//! Read-only Gateway client. Every call reads live; nothing is cached.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        let response = request
            .send()
            .await
            .map_err(|error| self.scrub(&format!("gateway request failed: {error}")))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(self.scrub(&format!(
                "gateway HTTP {}: {}",
                status,
                short(&body)
            )));
        }
        Ok(self.scrub(&body))
    }

    /// Live health answer from `/health`, trimmed for display.
    pub async fn health(&self) -> Result<String, String> {
        Ok(short(&self.get("/health").await?))
    }

    /// Live model catalogue from `/v1/models` (Gateway's `GET` route).
    pub async fn models(&self) -> Result<Vec<ModelInfo>, String> {
        let body = self.get("/v1/models").await?;
        parse_models(&body)
    }
}

fn short(body: &str) -> String {
    body.trim().chars().take(300).collect()
}

fn parse_models(body: &str) -> Result<Vec<ModelInfo>, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model catalogue: {error}"))?;
    let rows = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or("model catalogue has no models array")?;
    rows.iter()
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
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gateway_catalogue_rows() {
        let body = r#"{"models":[
            {"id":"local::gemma","display_name":"Gemma","context_window_tokens":262144},
            {"id":"openai::gpt","name":"GPT"}
        ],"freshness":{"stale":false}}"#;
        assert_eq!(
            parse_models(body).unwrap(),
            vec![
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
            ]
        );
    }
}
