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
    let text = serde_json::to_string(&body).unwrap_or_else(|_| "{\"error\":\"encode\"}".into());
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

fn value_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn body(request: &mut tiny_http::Request) -> Value {
    let mut raw = String::new();
    let _ = request.as_reader().take(1_048_576).read_to_string(&mut raw);
    serde_json::from_str(&raw).unwrap_or(Value::Null)
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

fn schedule(state: &Shared, payload: Value) -> (u16, Value) {
    let Some(text) = payload.get("text").and_then(Value::as_str) else {
        return (400, json!({ "error": "text required" }));
    };
    let targets: Vec<String> = payload
        .get("targets")
        .and_then(Value::as_array)
        .map(|array| array.iter().map(value_string).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    if targets.is_empty() {
        return (400, json!({ "error": "targets required" }));
    }
    // create_post owns dedupe and due time; delay_s is a test-side request only.
    let created = {
        let mut store = guard(&state.store);
        result_value(store.create_post(text, &targets).map_err(|error| error.to_string()))
    };
    let (post_id, delivery_ids) = match created {
        Ok(value) => created_ids(value),
        Err(error) => return (400, json!({ "error": error })),
    };
    (200, json!({ "post_id": post_id, "delivery_ids": delivery_ids }))
}

fn trigger(state: &Shared, payload: Value, retry: bool) -> (u16, Value) {
    let Some(id) = payload.get("id").and_then(Value::as_str).map(str::to_string) else {
        return (400, json!({ "error": "id required" }));
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
    match result {
        Ok(value) => {
            let ok = value != Value::Bool(false);
            (200, json!({ "ok": ok, "action": action, "id": id, "result": value }))
        }
        Err(error) => (400, json!({ "ok": false, "action": action, "id": id, "error": error })),
    }
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

        let given = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("x-wire-operator"))
            .map(|h| h.value.as_str().to_string())
            .unwrap_or_default();
        if !hold::token_eq(&given, &token) {
            respond(request, 403, json!({ "error": "guard: operator credential required" }));
            continue;
        }

        let payload = body(&mut request);
        let (status, reply) = match (method.as_str(), path.as_str()) {
            ("POST", "/api/test/trigger/schedule") => schedule(&state, payload),
            ("POST", "/api/test/trigger/cancel") => trigger(&state, payload, false),
            ("POST", "/api/test/trigger/retry") => trigger(&state, payload, true),
            _ => (404, json!({ "error": "not found" })),
        };
        respond(request, status, reply);
    }
    Ok(())
}
