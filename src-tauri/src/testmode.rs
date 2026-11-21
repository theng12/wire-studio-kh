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
    let cfg = crate::settings::load(&crate::settings::path_in(&state.data_dir));
    let sec = crate::secrets::load(&crate::secrets::path_in(&state.data_dir));
    let (connection, reason) = crate::settings::telegram_status(
        !sec.telegram.bot_token.is_empty(),
        &cfg.telegram_chat,
        &cfg.telegram_chat_title,
        cfg.telegram_verified.as_ref(),
        &cfg.telegram_verify_error,
    );
    let verified_bot = cfg
        .telegram_verified
        .as_ref()
        .map(|check| check.bot.clone())
        .unwrap_or_default();
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
        "version": env!("CARGO_PKG_VERSION"),
        "paused": state.paused.load(std::sync::atomic::Ordering::SeqCst),
        "in_flight": hold::in_flight(state),
        "deliveries": rows,
        "next_due": next_due,
        "telegram": {
            "configured": !sec.telegram.bot_token.is_empty() && !cfg.telegram_chat.is_empty(),
            "token_masked": if sec.telegram.bot_token.is_empty() { String::new() } else { crate::secrets::mask(&sec.telegram.bot_token) },
            "chat": cfg.telegram_chat,
            "chat_title": cfg.telegram_chat_title,
            "bot": verified_bot,
            "connection_state": connection,
            "reason": reason,
        },
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

/// Test-side scheduling: a positive delay moves the post's queued rows into the future.
fn delay_s(payload: &Value) -> i64 {
    payload
        .get("delay_s")
        .and_then(Value::as_i64)
        .filter(|seconds| *seconds > 0)
        .unwrap_or(0)
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
    let delay = delay_s(&payload);
    // Optional { "<target>": "<preset id>" } map — the agent equivalent of the
    // composer's preset picker, same validation as the UI path.
    let presets = payload.get("presets").and_then(Value::as_object).cloned().unwrap_or_default();
    // No entry means the platform's default preset, exactly what the composer pre-selects.
    let choices: Vec<(String, Option<String>)> = targets
        .iter()
        .map(|t| {
            let platform = t.split_once(':').map_or(t.as_str(), |(p, _)| p);
            let preset = presets
                .get(t)
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    crate::formats::presets_for(platform)
                        .first()
                        .map(|p| p.id.to_string())
                });
            (t.clone(), preset)
        })
        .collect();
    let created = {
        let mut store = guard(&state.store);
        let created = result_value(
            store
                .create_post_with_presets(text, &choices)
                .map_err(|error| error.to_string()),
        );
        if let Ok(value) = &created {
            let (post_id, _) = created_ids(value.clone());
            if delay > 0 {
                let at = chrono::Utc::now() + chrono::Duration::seconds(delay);
                if let Err(error) = store.set_next_attempt_for_post(&post_id, at) {
                    return (400, json!({ "ok": false, "error": format!("cannot delay post: {error}"), "post_id": post_id, "delivery_ids": [] }));
                }
            }
        }
        created
    };
    let (post_id, delivery_ids) = match created {
        Ok(value) => created_ids(value),
        Err(error) => {
            return (400, json!({ "ok": false, "error": error, "post_id": "", "delivery_ids": [] }))
        }
    };
    state.wake();
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
    state.wake();
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
/// The agent-friendly equivalent of the import screen: preview or run, same
/// guards, same code path as the Tauri commands.
fn import_channel(state: &AppState, payload: Value) -> (u16, Value) {
    let Some(root) = payload.get("root").and_then(Value::as_str).map(str::to_string) else {
        return (400, json!({ "ok": false, "error": "root required" }));
    };
    let apply = payload.get("apply").and_then(Value::as_bool).unwrap_or(true);
    let plan = match crate::import::plan::scan(std::path::Path::new(&root)) {
        Ok(plan) => plan,
        Err(error) => return (400, json!({ "ok": false, "error": error })),
    };
    if !apply {
        return (
            200,
            json!({
                "ok": true,
                "applied": false,
                "channel": plan.channel_slug,
                "posts": plan.posts.len(),
                "fonts": plan.fonts.len(),
                "skipped": plan.skipped,
            }),
        );
    }
    match crate::import::apply::apply(&plan, &state.data_dir) {
        Ok(report) => (200, json!({ "ok": true, "applied": true, "report": report })),
        Err(error) => (400, json!({ "ok": false, "error": error })),
    }
}

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

    // Same rule as the windowed path: saved is not connected until Telegram answers.
    let mut chat_title = String::new();
    let verification = match crate::adapters::telegram::check_blocking(&sec.telegram.bot_token, &chat) {
        Ok(check) => {
            chat_title = check.chat.clone();
            cfg.telegram_chat_title = chat_title.clone();
            cfg.telegram_verified = Some(crate::settings::TelegramCheck {
                bot: check.bot.clone(),
                chat: check.chat.clone(),
                at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
            cfg.telegram_verify_error.clear();
            json!({ "ok": true, "summary": format!("{} → {}", check.bot, check.chat) })
        }
        Err(error) => {
            cfg.telegram_verified = None;
            cfg.telegram_chat_title.clear();
            cfg.telegram_verify_error = error.clone();
            json!({ "ok": false, "error": error })
        }
    };
    if let Err(error) = crate::settings::save(&sp, &cfg) {
        return (400, json!({ "ok": false, "error": format!("cannot save telegram channel: {error}") }));
    }
    (
        200,
        json!({
            "ok": true,
            "configured": true,
            "chat": chat,
            "chat_title": chat_title,
            "token_masked": crate::secrets::mask(&sec.telegram.bot_token),
            "verification": verification,
        }),
    )
}

fn telegram_chat_json(chat: &crate::adapters::telegram::DiscoveredChat) -> Value {
    json!({ "id": chat.id.as_str(), "title": chat.title.as_str(), "kind": chat.kind.as_str() })
}

/// The headless twin of `commands::telegram_connected_as`: a blank token reuses the saved one.
fn telegram_connected_as(state: &AppState, payload: Value) -> (u16, Value) {
    let requested = payload
        .get("token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let secrets_path = crate::secrets::path_in(&state.data_dir);
    let mut sec = crate::secrets::load(&secrets_path);
    let effective = if requested.is_empty() {
        sec.telegram.bot_token.clone()
    } else {
        requested.clone()
    };
    if effective.is_empty() {
        return (400, json!({ "ok": false, "error": "bot token required" }));
    }
    let bot = match crate::adapters::telegram::connected_as_blocking(&effective) {
        Ok(bot) => bot,
        Err(error) => return (200, json!({ "ok": false, "error": error })),
    };
    let changed = !requested.is_empty() && sec.telegram.bot_token != effective;
    if changed {
        sec.telegram.bot_token = effective;
        if let Err(error) = crate::secrets::save(&secrets_path, &sec) {
            return (400, json!({ "ok": false, "error": format!("cannot save telegram token: {error}") }));
        }
    }
    let settings_path = crate::settings::path_in(&state.data_dir);
    let mut cfg = crate::settings::load(&settings_path);
    if changed {
        cfg.telegram_verified = None;
        cfg.telegram_verify_error.clear();
        cfg.telegram_chat_title.clear();
        if let Err(error) = crate::settings::save(&settings_path, &cfg) {
            return (400, json!({ "ok": false, "error": format!("cannot save telegram channel: {error}") }));
        }
    }
    let chat_title = cfg
        .telegram_verified
        .as_ref()
        .map(|check| check.chat.clone())
        .unwrap_or_default();
    (200, json!({ "ok": true, "bot": bot, "chat_title": chat_title }))
}

fn telegram_discover(state: &AppState) -> (u16, Value) {
    let token = crate::secrets::load(&crate::secrets::path_in(&state.data_dir))
        .telegram
        .bot_token;
    if token.is_empty() {
        return (200, json!({ "ok": false, "error": "Connect the bot first." }));
    }
    match crate::adapters::telegram::discover_blocking(&token) {
        Ok(chats) => (
            200,
            json!({ "ok": true, "chats": chats.iter().map(telegram_chat_json).collect::<Vec<_>>() }),
        ),
        Err(error) => (200, json!({ "ok": false, "error": error })),
    }
}

fn telegram_use_channel(state: &AppState, payload: Value) -> (u16, Value) {
    let text = payload.get("text").and_then(Value::as_str).unwrap_or("").trim();
    let token = crate::secrets::load(&crate::secrets::path_in(&state.data_dir))
        .telegram
        .bot_token;
    if token.is_empty() {
        return (200, json!({ "ok": false, "error": "Connect the bot first." }));
    }
    let chat = match crate::adapters::telegram::resolve_blocking(&token, text) {
        Ok(chat) => chat,
        Err(error) => return (200, json!({ "ok": false, "error": error })),
    };
    let bot = match crate::adapters::telegram::connected_as_blocking(&token) {
        Ok(bot) => bot,
        Err(error) => return (200, json!({ "ok": false, "error": error })),
    };
    let path = crate::settings::path_in(&state.data_dir);
    let mut cfg = crate::settings::load(&path);
    cfg.telegram_chat = chat.id.clone();
    cfg.telegram_chat_title = chat.title.clone();
    cfg.telegram_verified = Some(crate::settings::TelegramCheck {
        bot,
        chat: chat.title.clone(),
        at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    });
    cfg.telegram_verify_error.clear();
    if let Err(error) = crate::settings::save(&path, &cfg) {
        return (400, json!({ "ok": false, "error": format!("cannot save telegram channel: {error}") }));
    }
    (200, json!({ "ok": true, "chat": telegram_chat_json(&chat) }))
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
            ("POST", "/api/test/trigger/telegram-connected-as") => "telegram-connected-as",
            ("POST", "/api/test/trigger/telegram-discover") => "telegram-discover",
            ("POST", "/api/test/trigger/telegram-use-channel") => "telegram-use-channel",
            ("POST", "/api/test/trigger/import") => "import",
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
            "telegram-connected-as" => telegram_connected_as(&state, payload),
            "telegram-discover" => telegram_discover(&state),
            "telegram-use-channel" => telegram_use_channel(&state, payload),
            "import" => import_channel(&state, payload),
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

    #[test]
    fn schedule_fields() {
        assert_eq!(delay_s(&json!({"delay_s": 30})), 30);
        assert_eq!(delay_s(&json!({"delay_s": 0})), 0);
        assert_eq!(delay_s(&json!({"delay_s": -5})), 0);
        assert_eq!(delay_s(&json!({})), 0);
        assert_eq!(
            telegram_fields(&json!({"token": " t ", "chat": " @c "})),
            Ok(("t".to_string(), "@c".to_string()))
        );
        assert_eq!(telegram_fields(&json!({"token": "t"})), Err("chat required"));
        assert_eq!(telegram_fields(&json!({"chat": "  "})), Err("chat required"));
    }
}
