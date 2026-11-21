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

    let presets: Vec<serde_json::Value> = crate::formats::registry()
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "platform": p.platform,
                "label": p.label,
                "ratio": p.ratio,
                "max_text_length": p.max_text_length,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "paused": s.paused.load(Ordering::Relaxed),
        "in_flight": in_flight,
        "deliveries": rows,
        "next_due": next.map(|d| d.to_rfc3339()),
        "targets": all_targets,
        "presets": presets,
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
    presets: Option<std::collections::HashMap<String, String>>,
) -> Result<serde_json::Value, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Write something first.".into());
    }
    if targets.is_empty() {
        return Err("Pick at least one channel.".into());
    }
    let choices: Vec<(String, Option<String>)> = targets
        .iter()
        .map(|target| (target.clone(), presets.as_ref().and_then(|m| m.get(target)).cloned()))
        .collect();
    let created = {
        let mut store = s.store.lock().map_err(|_| "queue busy".to_string())?;
        let (post_id, ids) = store.create_post_with_presets(&text, &choices).map_err(err)?;
        if delay_s > 0 {
            let at = chrono::Utc::now() + chrono::Duration::seconds(delay_s);
            store.set_next_attempt_for_post(&post_id, at).map_err(err)?;
        }
        serde_json::json!({ "post_id": post_id, "delivery_ids": ids })
    };
    s.wake();
    Ok(created)
}

#[tauri::command]
pub fn cancel_delivery(s: State<'_, Shared>, id: String) -> Result<serde_json::Value, String> {
    let outcome = {
        let mut store = s.store.lock().map_err(|_| "queue busy".to_string())?;
        store.cancel(&id)
    };
    outcome.map_err(|e| format!("This post can no longer be cancelled: {e}"))?;
    s.wake();
    Ok(serde_json::json!({ "id": id, "state": "cancelled" }))
}

#[tauri::command]
pub fn retry_delivery(s: State<'_, Shared>, id: String) -> Result<serde_json::Value, String> {
    let outcome = {
        let mut store = s.store.lock().map_err(|_| "queue busy".to_string())?;
        store.retry(&id)
    };
    outcome.map_err(|e| format!("This post cannot be retried: {e}"))?;
    s.wake();
    Ok(serde_json::json!({ "id": id, "state": "queued" }))
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

/// The in-app folder browser behind every path field's Choose… button.
#[tauri::command]
pub fn list_dirs(path: Option<String>) -> Result<serde_json::Value, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let start = path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(&home));
    let start = start.canonicalize().unwrap_or(start);
    let mut dirs = Vec::new();
    let entries = std::fs::read_dir(&start).map_err(|e| format!("Cannot open {}: {e}", start.display()))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                dirs.push(serde_json::json!({ "name": name, "path": p.display().to_string() }));
            }
        }
    }
    dirs.sort_by(|a, b| a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or("")));
    Ok(serde_json::json!({
        "path": start.display().to_string(),
        "parent": start.parent().map(|p| p.display().to_string()),
        "dirs": dirs,
    }))
}

/// Read-only preview of a Story channel's Still Posts. Nothing is written.
#[tauri::command]
pub fn story_import_preview(root: String) -> Result<serde_json::Value, String> {
    let plan = crate::import::plan::scan(std::path::Path::new(&root))?;
    let summary = serde_json::json!({
        "channel": plan.channel_slug,
        "name": plan.channel_name,
        "posts": plan.posts.len(),
        "fonts": plan.fonts.len(),
        "skipped": plan.skipped.len(),
        "files_read": plan.source_files,
    });
    Ok(serde_json::json!({ "summary": summary, "plan": plan }))
}

/// Copies what the preview listed into Wire's data dir. Re-runs skip what landed.
#[tauri::command]
pub fn story_import_run(s: State<'_, Shared>, root: String) -> Result<serde_json::Value, String> {
    let plan = crate::import::plan::scan(std::path::Path::new(&root))?;
    let report = crate::import::apply::apply(&plan, &s.data_dir)?;
    Ok(serde_json::json!({ "report": report }))
}
