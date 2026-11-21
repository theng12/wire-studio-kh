//! Tauri commands. Every action here is the same code path the headless test
//! surface uses, so the UI and an agent cannot drift apart.

use std::sync::atomic::Ordering;

use tauri::State;

use crate::{feedcheck, secrets, settings, Shared};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn state(s: State<'_, Shared>) -> Result<serde_json::Value, String> {
    let cfg = settings::load(&settings::path_in(&s.data_dir));
    let sec = secrets::load(&secrets::path_in(&s.data_dir));
    let store = s.store.lock().map_err(|_| "queue busy".to_string())?;
    let rows = store.queue_view().map_err(err)?;
    let next = store.next_due().map_err(err)?;
    let in_flight = store.in_flight_count().map_err(err)?;
    let targets = store.targets_seen().unwrap_or_default();

    let mut all_targets = targets;
    if !cfg.telegram_chat.is_empty() {
        let t = format!("telegram:{}", cfg.telegram_chat);
        if !all_targets.contains(&t) {
            all_targets.push(t);
        }
    }

    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "paused": s.paused.load(Ordering::Relaxed),
        "in_flight": in_flight,
        "deliveries": rows,
        "next_due": next.map(|d| d.to_rfc3339()),
        "targets": all_targets,
        "telegram": {
            "configured": !sec.telegram.bot_token.is_empty() && !cfg.telegram_chat.is_empty(),
            "token_masked": if sec.telegram.bot_token.is_empty() { String::new() } else { secrets::mask(&sec.telegram.bot_token) },
            "chat": cfg.telegram_chat,
        },
        "appearance": cfg.appearance,
        "data_dir": s.data_dir.display().to_string(),
    }))
}

#[tauri::command]
pub fn queue_post(
    s: State<'_, Shared>,
    text: String,
    targets: Vec<String>,
    delay_s: i64,
) -> Result<serde_json::Value, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Write something first.".into());
    }
    if targets.is_empty() {
        return Err("Pick at least one channel.".into());
    }
    let mut store = s.store.lock().map_err(|_| "queue busy".to_string())?;
    let (post_id, ids) = store.create_post(&text, &targets).map_err(err)?;
    if delay_s > 0 {
        let at = chrono::Utc::now() + chrono::Duration::seconds(delay_s);
        store.set_next_attempt_for_post(&post_id, at).map_err(err)?;
    }
    Ok(serde_json::json!({ "post_id": post_id, "delivery_ids": ids }))
}

#[tauri::command]
pub fn cancel_delivery(s: State<'_, Shared>, id: String) -> Result<serde_json::Value, String> {
    let mut store = s.store.lock().map_err(|_| "queue busy".to_string())?;
    match store.cancel(&id) {
        Ok(()) => Ok(serde_json::json!({ "id": id, "state": "cancelled" })),
        Err(e) => Err(format!("This post can no longer be cancelled: {e}")),
    }
}

#[tauri::command]
pub fn retry_delivery(s: State<'_, Shared>, id: String) -> Result<serde_json::Value, String> {
    let mut store = s.store.lock().map_err(|_| "queue busy".to_string())?;
    match store.retry(&id) {
        Ok(()) => Ok(serde_json::json!({ "id": id, "state": "queued" })),
        Err(e) => Err(format!("This post cannot be retried: {e}")),
    }
}

#[tauri::command]
pub fn save_telegram(
    s: State<'_, Shared>,
    token: String,
    chat: String,
) -> Result<serde_json::Value, String> {
    let chat = chat.trim().to_string();
    if chat.is_empty() {
        return Err("Add the channel first (for example @my_channel).".into());
    }
    let path = secrets::path_in(&s.data_dir);
    let mut sec = secrets::load(&path);
    if !token.trim().is_empty() {
        sec.telegram.bot_token = token.trim().to_string();
    }
    if sec.telegram.bot_token.is_empty() {
        return Err("Add the bot token first.".into());
    }
    secrets::save(&path, &sec)?;

    let sp = settings::path_in(&s.data_dir);
    let mut cfg = settings::load(&sp);
    cfg.telegram_chat = chat.clone();
    settings::save(&sp, &cfg)?;

    Ok(serde_json::json!({
        "configured": true,
        "token_masked": secrets::mask(&sec.telegram.bot_token),
        "chat": chat,
    }))
}

#[tauri::command]
pub fn save_appearance(s: State<'_, Shared>, choice: String) -> Result<(), String> {
    if !["system", "light", "dark"].contains(&choice.as_str()) {
        return Err("Appearance must be system, light or dark.".into());
    }
    let sp = settings::path_in(&s.data_dir);
    let mut cfg = settings::load(&sp);
    cfg.appearance = choice;
    settings::save(&sp, &cfg)
}

#[tauri::command]
pub async fn check_updates() -> Result<feedcheck::UpdateState, String> {
    Ok(feedcheck::check(env!("CARGO_PKG_VERSION")).await)
}
