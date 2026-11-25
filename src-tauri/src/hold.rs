//! Update hold — the loopback side of the contract shared with the Launcher/Updater.
//!
//! The pause is one instant in memory: POST closes dispatch at once, waits a bounded moment for
//! work already inside, and DELETE (or expiry) opens dispatch again. Nothing is persisted, so a
//! killed process can never leave the next start refusing work.

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::AppState;

const POLL: Duration = Duration::from_millis(250);
const DEFAULT_TIMEOUT_S: u64 = 20;
const MAX_TIMEOUT_S: u64 = 30;
const DEFAULT_PAUSE_S: u64 = 120;
const MIN_PAUSE_S: u64 = 1;
const MAX_PAUSE_S: u64 = 900;

type Shared = Arc<AppState>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenDecision {
    Allowed,
    Refused,
}

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

fn header(request: &tiny_http::Request, name: &'static str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().to_string())
}

/// Wire's own token file, beside the app's data. The estate path below is retired in 0.5.2.
const BUNDLE_ID: &str = "com.terranash.wirestudiokh";

fn token_path(home: &Path) -> std::path::PathBuf {
    home.join("Library/Application Support").join(BUNDLE_ID).join("hold-token")
}

fn legacy_token_path(home: &Path) -> std::path::PathBuf {
    home.join("Library/KHUpdater/hold-token")
}

fn read_token(path: &Path) -> Option<String> {
    let token = std::fs::read_to_string(path).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// Wire's own file first. The estate path is copied here once, said out loud, and never read again;
/// when neither exists the answer is None and the route refuses with 401.
fn launcher_token() -> Option<String> {
    let home = std::path::PathBuf::from(std::env::var("HOME").ok()?);
    let own = token_path(&home);
    if let Some(token) = read_token(&own) {
        return Some(token);
    }
    let legacy = legacy_token_path(&home);
    let token = read_token(&legacy)?;
    if let Some(dir) = own.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&own, format!("{token}\n")).ok()?;
    let _ = std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o600));
    eprintln!(
        "hold token moved from {} to {}; that file is not read again",
        legacy.display(),
        own.display()
    );
    Some(token)
}

/// What the window says about the token, in the app's own words. Existence only for the estate path:
/// its content is read solely by the one migration in `launcher_token`.
pub fn token_status_json() -> serde_json::Value {
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let own = token_path(&home);
    if read_token(&own).is_some() {
        return json!({ "state": "ready", "path": own.display().to_string() });
    }
    if legacy_token_path(&home).is_file() {
        return json!({
            "state": "moves",
            "path": own.display().to_string(),
            "reason": format!(
                "The Launcher's token at {} is copied to {} the first time an update is held.",
                legacy_token_path(&home).display(),
                own.display()
            ),
        });
    }
    json!({
        "state": "missing",
        "path": own.display().to_string(),
        "reason": format!(
            "No update-hold token at {}. Updates are refused until the Launcher writes one.",
            own.display()
        ),
    })
}

/// Seconds from the query: 20 default / 30 ceiling for the settle, 120 default for the pause.
fn parse_hold_query(query: &str) -> (u64, u64) {
    let param = |key: &str| {
        query
            .trim_start_matches('?')
            .split('&')
            .find_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                (name == key)
                    .then(|| value.parse::<f64>().ok())
                    .flatten()
                    .filter(|value| value.is_finite())
                    .map(|value| value.max(0.0) as u64)
            })
    };
    (
        param("timeout_s").unwrap_or(DEFAULT_TIMEOUT_S).min(MAX_TIMEOUT_S),
        param("pause_s")
            .unwrap_or(DEFAULT_PAUSE_S)
            .clamp(MIN_PAUSE_S, MAX_PAUSE_S),
    )
}

/// A browser must not be able to pause the app through loopback; no Origin is fine.
fn origin_allowed(origin: Option<&str>) -> bool {
    matches!(origin, None | Some("tauri://localhost"))
}

/// A missing or empty token file is a refusal, never a skipped hold (issue row 20261123-77): an
/// unauthenticated caller must never receive a green light before Sparkle checks the feed.
fn token_decision(header: Option<&str>, expected: Option<&str>) -> TokenDecision {
    match expected.filter(|token| !token.is_empty()) {
        None => TokenDecision::Refused,
        Some(expected) if token_eq(header.unwrap_or(""), expected) => TokenDecision::Allowed,
        Some(_) => TokenDecision::Refused,
    }
}

/// `Authorization: Bearer …` is how the Sparkle shim presents the same token (INTEGRATION §0).
fn bearer_token(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let rest = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?;
    let token = rest.trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn pause_expired(paused_until: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    paused_until.map_or(false, |until| until <= now)
}

/// Expire the in-memory pause before any request is handled.
pub(crate) fn refresh_pause(state: &AppState) {
    let mut until = guard(&state.paused_until);
    if pause_expired(*until, Utc::now()) {
        state.paused.store(false, Ordering::SeqCst);
        *until = None;
        state.wake();
    }
}

fn respond(request: tiny_http::Request, status: u16, body: Value) {
    let text = serde_json::to_string(&body).unwrap_or_else(|_| "{\"ok\":false,\"error\":\"encode\"}".into());
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
pub(crate) fn sending_ids(state: &AppState) -> Vec<String> {
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
            .filter_map(|row| row.get("id").and_then(Value::as_str))
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// Settled means zero in flight before the deadline; unsettled carries ids, never prose.
fn settle_result(count: usize, ids: Vec<String>, timed_out: bool) -> (bool, Vec<String>) {
    (!timed_out && count == 0 && ids.is_empty(), ids)
}

fn hold_reply(safe: bool, unsettled: Vec<String>, count: usize, pause_s: u64) -> Value {
    json!({
        "safe": safe,
        "unsettled": unsettled,
        "dispatch_paused_s": pause_s,
        "in_flight": count,
    })
}

fn take_hold(state: &Shared, query: &str) -> Value {
    let (timeout_s, pause_s) = parse_hold_query(query);
    let until = Utc::now() + chrono::Duration::seconds(pause_s as i64);

    // Close dispatch before waiting: anything that arrives during the wait stays queued.
    state.paused.store(true, Ordering::SeqCst);
    *guard(&state.paused_until) = Some(until);

    let deadline = Instant::now() + Duration::from_secs(timeout_s);
    let (count, ids, timed_out) = loop {
        let ids = sending_ids(state);
        let count = in_flight(state);
        if count == 0 && ids.is_empty() {
            break (count, ids, false);
        }
        let now = Instant::now();
        if now >= deadline {
            break (count, ids, true);
        }
        thread::sleep(POLL.min(deadline - now));
    };
    let (safe, unsettled) = settle_result(count, ids, timed_out);
    eprintln!(
        "update hold: {} in_flight={} pause_s={}",
        if safe { "settled" } else { "unsettled" },
        count,
        pause_s
    );
    hold_reply(safe, unsettled, count, pause_s)
}

fn release(state: &Shared) -> Value {
    state.paused.store(false, Ordering::SeqCst);
    *guard(&state.paused_until) = None;
    state.wake();
    json!({ "resumed": true })
}

/// Bind loopback only. A bind failure is returned to lib.rs, which logs and fails open.
pub fn serve(state: Shared, port: u16) -> Result<(), String> {
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
    for request in server.incoming_requests() {
        refresh_pause(&state);

        if !origin_allowed(header(&request, "Origin").as_deref()) {
            respond(request, 403, json!({ "error": "hold refused: origin" }));
            continue;
        }

        let expected = launcher_token();
        let presented = header(&request, "x-hold-token")
            .or_else(|| bearer_token(header(&request, "authorization").as_deref()));
        let decision = token_decision(presented.as_deref(), expected.as_deref());
        if decision == TokenDecision::Refused {
            // 401 with nothing paused: the caller then refuses its own update.
            respond(request, 401, json!({ "error": "hold refused: token" }));
            continue;
        }

        let (path, query) = request.url().split_once('?').unwrap_or((request.url(), ""));
        if path != "/api/maintenance/hold" {
            respond(request, 404, json!({ "error": "not found" }));
            continue;
        }
        let (status, body) = match request.method().as_str().to_uppercase().as_str() {
            "POST" => (200, take_hold(&state, query)),
            "DELETE" => (200, release(&state)),
            _ => (405, json!({ "error": "hold refused: method" })),
        };
        respond(request, status, body);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn query_defaults_and_clamps() {
        assert_eq!(parse_hold_query(""), (DEFAULT_TIMEOUT_S, DEFAULT_PAUSE_S));
        assert_eq!(parse_hold_query("?timeout_s=5&pause_s=60"), (5, 60));
        assert_eq!(parse_hold_query("timeout_s=999&pause_s=9999"), (MAX_TIMEOUT_S, MAX_PAUSE_S));
        assert_eq!(parse_hold_query("timeout_s=-4&pause_s=0"), (0, MIN_PAUSE_S));
        assert_eq!(parse_hold_query("timeout_s=nope&pause_s=NaN"), (DEFAULT_TIMEOUT_S, DEFAULT_PAUSE_S));
        assert_eq!(parse_hold_query("?timeout_s=1e3&pause_s=1.9"), (MAX_TIMEOUT_S, 1));
    }

    #[test]
    fn origin_table() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some("tauri://localhost")));
        assert!(!origin_allowed(Some("https://evil.example")));
        assert!(!origin_allowed(Some("tauri://localhost/")));
        assert!(!origin_allowed(Some("TAURI://localhost")));
    }

    #[test]
    fn token_table() {
        // No token file is a refusal, not a skipped hold (row 20261123-77).
        assert_eq!(token_decision(None, None), TokenDecision::Refused);
        assert_eq!(token_decision(Some("x"), None), TokenDecision::Refused);
        assert_eq!(token_decision(Some("x"), Some("")), TokenDecision::Refused);
        assert_eq!(token_decision(Some("abc"), Some("abc")), TokenDecision::Allowed);
        assert_eq!(token_decision(None, Some("abc")), TokenDecision::Refused);
        assert_eq!(token_decision(Some("abc"), Some("ABD")), TokenDecision::Refused);
        assert!(!token_eq("abc", ""));
    }

    #[test]
    fn bearer_token_table() {
        assert_eq!(bearer_token(Some("Bearer abc")), Some("abc".to_string()));
        assert_eq!(bearer_token(Some("bearer  abc ")), Some("abc".to_string()));
        assert_eq!(bearer_token(Some("Basic abc")), None);
        assert_eq!(bearer_token(Some("Bearer ")), None);
        assert_eq!(bearer_token(None), None);
    }

    #[test]
    fn pause_expires_after_the_requested_pause() {
        let (_, pause_s) = parse_hold_query("?pause_s=120");
        let now = Utc.with_ymd_and_hms(2026, 11, 21, 1, 2, 3).unwrap();
        let until = now + chrono::Duration::seconds(pause_s as i64);
        assert!(!pause_expired(Some(until), now));
        assert!(!pause_expired(Some(until), now + chrono::Duration::seconds(pause_s as i64 - 1)));
        assert!(pause_expired(Some(until), now + chrono::Duration::seconds(pause_s as i64)));
        assert!(pause_expired(Some(now), now));
        assert!(!pause_expired(None, now + chrono::Duration::days(1)));
    }

    #[test]
    fn reply_shape_and_unsettled_ids() {
        let reply = hold_reply(true, settle_result(0, Vec::new(), false).1, 0, 120);
        let mut keys: Vec<&str> = reply.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["dispatch_paused_s", "in_flight", "safe", "unsettled"]);
        assert_eq!(reply["safe"].as_bool(), Some(true));
        assert_eq!(reply["in_flight"].as_u64(), Some(0));
        assert_eq!(reply["dispatch_paused_s"].as_u64(), Some(120));

        let ids = vec!["delivery-1".to_string(), "delivery-2".to_string()];
        let (safe, unsettled) = settle_result(2, ids.clone(), true);
        assert!(!safe);
        assert_eq!(unsettled, ids);
        let reply = hold_reply(safe, unsettled, 2, 20);
        assert_eq!(reply["unsettled"], json!(ids));
        assert!(reply["unsettled"].as_array().unwrap().iter().all(Value::is_string));
        assert!(!settle_result(0, Vec::new(), true).0);
        assert!(!settle_result(0, ids, false).0);
    }
}
