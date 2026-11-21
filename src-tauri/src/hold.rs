//! Update hold — the loopback side of the contract shared with the Launcher/Updater.
//!
//! The pause is one instant in memory: POST closes dispatch at once, waits a bounded moment for
//! work already inside, and DELETE (or expiry) opens dispatch again. Nothing is persisted, so a
//! killed process can never leave the next start refusing work.

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::AppState;

const POLL: Duration = Duration::from_millis(250);
const DEFAULT_TIMEOUT_S: f64 = 20.0;
const MAX_TIMEOUT_S: f64 = 30.0;
const DEFAULT_PAUSE_S: f64 = 120.0;
static WARNED_NO_TOKEN: AtomicBool = AtomicBool::new(false);

type Shared = Arc<AppState>;

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Constant time compare of two credentials; an empty expected token matches nothing.
pub(crate) fn token_eq(actual: &str, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let (a, b) = (actual.as_bytes(), expected.as_bytes());
    let mut diff = 0u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

fn header<'a>(request: &'a tiny_http::Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
}

/// The Launcher rotates this file; read it on every call, never cache it.
fn launcher_token() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let token = std::fs::read_to_string(Path::new(&home).join("Library/KHUpdater/hold-token")).ok()?;
    let token = token.trim().to_string();
    (!token.is_empty()).then_some(token)
}

fn bounded(value: Option<f64>, default: f64, min: f64, max: f64) -> f64 {
    match value {
        Some(value) if value.is_finite() => value.clamp(min, max),
        _ => default,
    }
}

fn number_param(url: &str, key: &str) -> Option<f64> {
    let (_, query) = url.split_once('?')?;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| value.parse::<f64>().ok()).flatten()
    })
}

/// Expire the in-memory pause before any request is handled.
pub(crate) fn refresh_pause(state: &AppState) {
    let expired = (*guard(&state.paused_until))
        .map(|until: DateTime<Utc>| until <= Utc::now())
        .unwrap_or(false);
    if expired {
        state.paused.store(false, Ordering::SeqCst);
        *guard(&state.paused_until) = None;
    }
}

fn respond(request: tiny_http::Request, status: u16, body: Value) {
    let text = serde_json::to_string(&body).unwrap_or_else(|_| "{\"error\":\"encode\"}".into());
    let mut response = tiny_http::Response::from_string(text).with_status_code(status);
    response.add_header(tiny_http::Header::from_bytes("Content-Type", "application/json").expect("header"));
    let _ = request.respond(response);
}

/// The queue is authoritative: dispatch claims rows in SQLite, so the atomic alone would say 0.
pub(crate) fn in_flight(state: &AppState) -> usize {
    let queued = {
        let store = guard(&state.store);
        store.in_flight_count().unwrap_or(0)
    };
    queued.max(state.in_flight.load(Ordering::SeqCst))
}

/// Ids of the rows actually sending, taken from the queue rather than guessed from the counter.
fn sending_ids(state: &AppState) -> Vec<String> {
    let view = {
        let store = guard(&state.store);
        serde_json::to_value(store.queue_view().unwrap_or_default()).unwrap_or(Value::Null)
    };
    let rows = view
        .as_array()
        .or_else(|| view.get("deliveries").and_then(Value::as_array));
    rows.map(|rows| {
        rows.iter()
            .filter(|row| row.get("state").and_then(Value::as_str) == Some("sending"))
            .map(|row| match row.get("id") {
                Some(Value::String(id)) => id.clone(),
                Some(id) => id.to_string(),
                None => "delivery (no id)".to_string(),
            })
            .collect()
    })
    .unwrap_or_default()
}

fn take_hold(state: &Shared, url: &str) -> Value {
    let pause_s = bounded(number_param(url, "pause_s"), DEFAULT_PAUSE_S, 1.0, 900.0);
    let timeout_s = bounded(number_param(url, "timeout_s"), DEFAULT_TIMEOUT_S, 0.0, MAX_TIMEOUT_S);
    let until = Utc::now() + chrono::Duration::milliseconds((pause_s * 1000.0) as i64);

    // Close dispatch before waiting: anything that arrives during the wait stays queued.
    state.paused.store(true, Ordering::SeqCst);
    *guard(&state.paused_until) = Some(until);

    let deadline = Instant::now() + Duration::from_secs_f64(timeout_s);
    let (mut ids, mut count);
    loop {
        ids = sending_ids(state);
        count = in_flight(state);
        if (count == 0 && ids.is_empty()) || Instant::now() >= deadline {
            break;
        }
        thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
    }

    let unsettled = if count == 0 {
        Vec::new()
    } else if ids.is_empty() {
        vec![format!("in_flight={count} (no delivery id)")]
    } else {
        ids
    };
    let safe = count == 0;
    eprintln!(
        "update hold: {} in_flight={} pause_s={}",
        if safe { "settled" } else { "unsettled" },
        count,
        pause_s
    );
    json!({
        "safe": safe,
        "unsettled": unsettled,
        "dispatch_paused_s": pause_s,
        "in_flight": count,
    })
}

fn release(state: &Shared) -> Value {
    state.paused.store(false, Ordering::SeqCst);
    *guard(&state.paused_until) = None;
    json!({ "resumed": true })
}

/// Bind loopback only. A bind failure is returned to lib.rs, which logs and fails open.
pub fn serve(state: Shared, port: u16) -> Result<(), String> {
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
    for request in server.incoming_requests() {
        refresh_pause(&state);

        // A web page must not be able to pause the app through loopback (L108).
        if let Some(origin) = header(&request, "Origin").map(str::to_string) {
            if origin != "tauri://localhost" {
                respond(request, 403, json!({ "error": "hold refused: origin" }));
                continue;
            }
        }

        if let Some(expected) = launcher_token() {
            if !token_eq(header(&request, "x-hold-token").unwrap_or(""), &expected) {
                respond(request, 403, json!({ "error": "hold refused: token" }));
                continue;
            }
        } else if WARNED_NO_TOKEN
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            eprintln!("hold unavailable — no Launcher token; update will be refused while running");
        }

        let path = request.url().split('?').next().unwrap_or("").to_string();
        if path != "/api/maintenance/hold" {
            respond(request, 404, json!({ "error": "not found" }));
            continue;
        }
        let url = request.url().to_string();
        let method = request.method().as_str().to_uppercase();
        match method.as_str() {
            "POST" => {
                let body = take_hold(&state, &url);
                respond(request, 200, body);
            }
            "DELETE" => respond(request, 200, release(&state)),
            _ => respond(request, 405, json!({ "error": "hold refused: method" })),
        }
    }
    Ok(())
}
