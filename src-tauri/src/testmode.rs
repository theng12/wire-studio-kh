//! Headless entry: the same guarded actions the UI and the agent use, with no window.
//!
//! Start with `--test-mode --KH_TEST_PORT <port>`; the printed URL and token drive the scripts in
//! `scripts/`. Nothing here opens a webview, and every answer is a small terminal JSON object.

use crate::{hold, AppState, Shared};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::Read;
use std::sync::{Mutex, MutexGuard};

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn respond(request: tiny_http::Request, status: u16, body: Value) {
    let text = serde_json::to_string(&body).unwrap_or_else(|_| "{\"ok\":false,\"error\":\"encode\"}".into());
    let mut response = tiny_http::Response::from_string(text).with_status_code(status);
    response.add_header(tiny_http::Header::from_bytes("Content-Type", "application/json").expect("header"));
    let _ = request.respond(response);
}

/// One scoped operator credential from the 0600 secrets file; WIRE_OPERATOR_TOKEN only overrides
/// it for headless scripts. No ambient loopback trust and no Keychain.
fn operator_token(state: &AppState) -> String {
    if let Ok(token) = std::env::var("WIRE_OPERATOR_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return token;
        }
    }
    let path = crate::secrets::path_in(&state.data_dir);
    let mut secrets = crate::secrets::load(&path);
    let token = secrets.ensure_operator_token().to_string();
    if let Err(error) = crate::secrets::save(&path, &secrets) {
        panic!("cannot save operator token {}: {error}", path.display());
    }
    token
}

/// Constant-time credential check; a missing header or empty token never grants access.
fn guard_ok(header: Option<&str>, token: &str) -> bool {
    hold::token_eq(header.unwrap_or(""), token)
}

fn value_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Parsing never panics: every bad body gets a 400 carrying the reason.
fn parse_body(raw: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(raw).map_err(|error| format!("invalid JSON body: {error}"))?;
    if !value.is_object() {
        return Err("body must be a JSON object".into());
    }
    Ok(value)
}

fn body(request: &mut tiny_http::Request) -> Result<Value, String> {
    let mut raw = String::new();
    request
        .as_reader()
        .take(1_048_576)
        .read_to_string(&mut raw)
        .map_err(|error| format!("cannot read body: {error}"))?;
    parse_body(&raw)
}

fn rows(state: &AppState) -> Vec<Value> {
    let view = {
        let store = guard(&state.store);
        serde_json::to_value(store.queue_view().unwrap_or_default()).unwrap_or(Value::Null)
    };
    view.as_array()
        .or_else(|| view.get("deliveries").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default()
}

fn state_reply(state: &Shared) -> Value {
    hold::refresh_pause(state);
    let rows = rows(state);
    let mut next_due: Option<String> = None;
    for row in &rows {
        for key in ["next_due", "next_attempt_at", "due_at"] {
            if let Some(due) = row.get(key).and_then(Value::as_str) {
                if next_due.as_ref().map_or(true, |current| due < current.as_str()) {
                    next_due = Some(due.to_string());
                }
            }
        }
    }
    json!({
        "ok": true,
        "paused": state.paused.load(std::sync::atomic::Ordering::SeqCst),
        "in_flight": hold::in_flight(state),
        "deliveries": rows,
        "next_due": next_due,
    })
}

/// `Result` serializes as `{"Ok":...}` / `{"Err":...}`; a plain value passes through.
fn result_value<T: Serialize>(result: T) -> Result<Value, String> {
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    if let Value::Object(map) = &value {
        if let Some(error) = map.get("Err") {
            return Err(value_string(error));
        }
        if let Some(ok) = map.get("Ok") {
            return Ok(ok.clone());
        }
    }
    Ok(value)
}

fn created_ids(value: Value) -> (String, Vec<String>) {
    if let Value::Array(parts) = &value {
        if parts.len() >= 2 {
            let ids = parts[1]
                .as_array()
                .map(|array| array.iter().map(value_string).collect())
                .unwrap_or_default();
            return (value_string(&parts[0]), ids);
        }
    }
    let post_id = value
        .get("post_id")
        .or_else(|| value.get("postId"))
        .map(value_string)
        .unwrap_or_default();
    let ids = value
        .get("delivery_ids")
        .or_else(|| value.get("deliveryIds"))
        .and_then(Value::as_array)
        .map(|array| array.iter().map(value_string).collect())
        .unwrap_or_default();
    (post_id, ids)
}

/// Cancel and retry name a delivery; report its post and delivery ids like schedule does.
fn ids_for(state: &AppState, id: &str) -> (String, Vec<String>) {
    let row = rows(state)
        .into_iter()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(id));
    let post_id = row
        .as_ref()
        .and_then(|row| row.get("post_id"))
        .map(value_string)
        .unwrap_or_default();
    (post_id, row.map(|_| vec![id.to_string()]).unwrap_or_default())
}

fn schedule(state: &Shared, payload: Value) -> (u16, Value) {
    let Some(text) = payload.get("text").and_then(Value::as_str) else {
        return (400, json!({ "ok": false, "error": "text required", "post_id": "", "delivery_ids": [] }));
    };
    let targets: Vec<String> = payload
        .get("targets")
        .and_then(Value::as_array)
        .map(|array| array.iter().map(value_string).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    if targets.is_empty() {
        return (400, json!({ "ok": false, "error": "targets required", "post_id": "", "delivery_ids": [] }));
    }
    // create_post owns dedupe and due time; delay_s is a test-side request only.
    let created = {
        let mut store = guard(&state.store);
        result_value(store.create_post(text, &targets).map_err(|error| error.to_string()))
    };
    let (post_id, delivery_ids) = match created {
        Ok(value) => created_ids(value),
        Err(error) => {
            return (400, json!({ "ok": false, "error": error, "post_id": "", "delivery_ids": [] }))
        }
    };
    (200, json!({ "ok": true, "post_id": post_id, "delivery_ids": delivery_ids }))
}

fn trigger(state: &Shared, payload: Value, retry: bool) -> (u16, Value) {
    let Some(id) = payload.get("id").and_then(Value::as_str).map(str::to_string) else {
        return (400, json!({ "ok": false, "error": "id required", "post_id": "", "delivery_ids": [] }));
    };
    let action = if retry { "retry" } else { "cancel" };
    let result = {
        let mut store = guard(&state.store);
        if retry {
            result_value(store.retry(&id).map_err(|error| error.to_string()))
        } else {
            result_value(store.cancel(&id).map_err(|error| error.to_string()))
        }
    };
    let (post_id, delivery_ids) = ids_for(state, &id);
    match result {
        Ok(value) => {
            let ok = value != Value::Bool(false);
            (
                200,
                json!({
                    "ok": ok,
                    "action": action,
                    "id": id,
                    "post_id": post_id,
                    "delivery_ids": delivery_ids,
                    "result": value,
                }),
            )
        }
        Err(error) => (
            400,
            json!({
                "ok": false,
                "action": action,
                "id": id,
                "post_id": post_id,
                "delivery_ids": delivery_ids,
                "error": error,
            }),
        ),
    }
}

/// The Targets dialog's headless equivalent: trim both fields; an empty token keeps the stored one.
fn telegram_fields(payload: &Value) -> Result<(String, String), &'static str> {
    let token = payload
        .get("token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let chat = payload
        .get("chat")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if chat.is_empty() {
        return Err("chat required");
    }
    Ok((token, chat))
}

/// Same secrets and settings helpers as `commands::save_telegram`, without a Tauri window.
fn save_telegram(state: &AppState, payload: Value) -> (u16, Value) {
    let (token, chat) = match telegram_fields(&payload) {
        Ok(fields) => fields,
        Err(reason) => return (400, json!({ "ok": false, "error": reason })),
    };
    let path = crate::secrets::path_in(&state.data_dir);
    let mut sec = crate::secrets::load(&path);
    if !token.is_empty() {
        sec.telegram.bot_token = token;
    }
    if sec.telegram.bot_token.is_empty() {
        return (400, json!({ "ok": false, "error": "bot token required" }));
    }
    if let Err(error) = crate::secrets::save(&path, &sec) {
        return (400, json!({ "ok": false, "error": format!("cannot save telegram token: {error}") }));
    }

    let sp = crate::settings::path_in(&state.data_dir);
    let mut cfg = crate::settings::load(&sp);
    cfg.telegram_chat = chat.clone();
    if let Err(error) = crate::settings::save(&sp, &cfg) {
        return (400, json!({ "ok": false, "error": format!("cannot save telegram channel: {error}") }));
    }
    (
        200,
        json!({
            "ok": true,
            "configured": true,
            "chat": chat,
            "token_masked": crate::secrets::mask(&sec.telegram.bot_token),
        }),
    )
}

/// Bind loopback only; a bind failure is returned to main.rs instead of opening a window.
pub fn serve(state: Shared, port: u16) -> Result<(), String> {
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
    let token = operator_token(&state);
    println!("TEST_URL=http://127.0.0.1:{port}");
    println!("TEST_TOKEN={token}");
    println!("TEST_DATA_DIR={}", state.data_dir.display());

    for mut request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("").to_string();
        let method = request.method().as_str().to_uppercase();
        if path == "/api/test/state" && method == "GET" {
            let reply = state_reply(&state);
            respond(request, 200, reply);
            continue;
        }

        let action = match (method.as_str(), path.as_str()) {
            ("POST", "/api/test/trigger/schedule") => "schedule",
            ("POST", "/api/test/trigger/cancel") => "cancel",
            ("POST", "/api/test/trigger/retry") => "retry",
            ("POST", "/api/test/trigger/save-telegram") => "save-telegram",
            _ => {
                let reason = format!("unknown route: {} {}", method.as_str(), path.as_str());
                respond(request, 404, json!({ "ok": false, "error": reason }));
                continue;
            }
        };

        let given = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("x-wire-operator"))
            .map(|h| h.value.as_str().to_string());
        if !guard_ok(given.as_deref(), &token) {
            respond(request, 403, json!({ "ok": false, "error": "guard: operator credential required" }));
            continue;
        }

        let payload = match body(&mut request) {
            Ok(value) => value,
            Err(reason) => {
                respond(request, 400, json!({ "ok": false, "error": reason }));
                continue;
            }
        };
        let (status, reply) = match action {
            "schedule" => schedule(&state, payload),
            "cancel" => trigger(&state, payload, false),
            "save-telegram" => save_telegram(&state, payload),
            _ => trigger(&state, payload, true),
        };
        respond(request, status, reply);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_guard() {
        assert!(guard_ok(Some("secret"), "secret"));
        assert!(!guard_ok(None, "secret"));
        assert!(!guard_ok(Some(""), "secret"));
        assert!(!guard_ok(Some("wrong"), "secret"));
        assert!(!guard_ok(Some(""), ""));
        assert!(!guard_ok(Some("secret"), "secret "));
    }

    #[test]
    fn malformed_bodies_carry_a_reason() {
        assert!(parse_body("").unwrap_err().contains("invalid JSON body"));
        assert!(parse_body("{\"id\":").unwrap_err().contains("invalid JSON body"));
        assert!(parse_body("[]").unwrap_err().contains("body must be a JSON object"));
        assert!(parse_body("null").unwrap_err().contains("body must be a JSON object"));
        assert_eq!(parse_body("{\"text\":\"x\"}").unwrap()["text"], "x");
    }
}
