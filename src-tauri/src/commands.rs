//! Tauri commands. Every action here is the same code path the headless test
//! surface uses, so the UI and an agent cannot drift apart.

use std::sync::atomic::Ordering;

use tauri::State;

use crate::adapters::telegram::{DiscoveredChat, TelegramAdapter};
use crate::adapters::AdapterError;
use crate::{feedcheck, secrets, settings, Shared};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn telegram_chat_json(chat: &DiscoveredChat) -> serde_json::Value {
    serde_json::json!({ "id": chat.id.as_str(), "title": chat.title.as_str(), "kind": chat.kind.as_str() })
}

/// One bound for every Telegram read; a timeout is a plain reason, not a hung action.
async fn bounded_telegram<T>(
    call: impl std::future::Future<Output = Result<T, AdapterError>>,
) -> Result<T, String> {
    tokio::time::timeout(std::time::Duration::from_secs(10), call)
        .await
        .map_err(|_| "Telegram did not answer within 10 seconds".to_string())?
        .map_err(|error| error.message)
}

fn saved_telegram_token(s: &Shared) -> String {
    secrets::load(&secrets::path_in(&s.data_dir)).telegram.bot_token.clone()
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

    // Saved is not connected: only a getMe+getChat answer may say "connected".
    let (connection, reason) = settings::telegram_status(
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


    let presets: Vec<serde_json::Value> = crate::formats::registry()
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "platform": p.platform,
                "label": p.label,
                "ratio": p.ratio,
                "max_text_length": p.max_text_length,
                "media": p.media,
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
            "chat_title": cfg.telegram_chat_title,
            "bot": verified_bot,
            "connection_state": connection,
            "reason": reason,
        },
        "gateway": crate::gateway::status_json(&s.data_dir),
        // Capture only: WIRE_START_SECTION opens a named section first, so docs/screenshots can be
        // taken from the shipped build without driving the pointer.
        "start_section": std::env::var("WIRE_START_SECTION").unwrap_or_default(),
        "media": crate::media::status_json(&s.data_dir),
        "hold": crate::hold::token_status_json(),
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
    product_id: Option<String>,
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
    // The price shown at compose time comes from Media now, read-only. A product that cannot be
    // read is still recorded by id: the send path then says the price is unavailable, with why.
    let product = product_id
        .as_deref()
        .map(|id| (id, crate::media::product(&s.data_dir, id).ok().and_then(|p| p.price_retail)));
    let created = {
        let mut store = s.store.lock().map_err(|_| "queue busy".to_string())?;
        let (post_id, ids) = store.create_post_with_presets(&text, &choices, product).map_err(err)?;
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
pub async fn save_telegram(
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

    // Saving proves nothing, so ask Telegram now: who is the token, can it see the channel.
    let adapter = crate::adapters::telegram::TelegramAdapter::new(sec.telegram.bot_token.clone());
    let answer = tokio::time::timeout(std::time::Duration::from_secs(10), adapter.check(&chat)).await;
    let mut chat_title = String::new();
    let verification = match answer {
        Ok(Ok(check)) => {
            chat_title = check.chat.clone();
            cfg.telegram_chat_title = chat_title.clone();
            cfg.telegram_verified = Some(settings::TelegramCheck {
                bot: check.bot.clone(),
                chat: check.chat.clone(),
                at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
            cfg.telegram_verify_error.clear();
            serde_json::json!({ "ok": true, "summary": format!("{} → {}", check.bot, check.chat) })
        }
        Ok(Err(error)) => {
            cfg.telegram_verified = None;
            cfg.telegram_chat_title.clear();
            cfg.telegram_verify_error = error.message.clone();
            serde_json::json!({ "ok": false, "error": error.message })
        }
        Err(_) => {
            cfg.telegram_verified = None;
            cfg.telegram_chat_title.clear();
            cfg.telegram_verify_error = "Telegram did not answer within 10 seconds".into();
            serde_json::json!({ "ok": false, "error": cfg.telegram_verify_error })
        }
    };
    settings::save(&sp, &cfg)?;

    Ok(serde_json::json!({
        "configured": true,
        "token_masked": secrets::mask(&sec.telegram.bot_token),
        "chat": chat,
        "chat_title": chat_title,
        "verification": verification,
    }))
}

/// Connect proves the token with getMe, then keeps it for the other two verbs.
#[tauri::command]
pub async fn telegram_connected_as(
    s: State<'_, Shared>,
    token: String,
) -> Result<serde_json::Value, String> {
    let requested = token.trim().to_string();
    let secrets_path = secrets::path_in(&s.data_dir);
    let mut sec = secrets::load(&secrets_path);
    let effective = if requested.is_empty() {
        sec.telegram.bot_token.clone()
    } else {
        requested.clone()
    };
    if effective.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "error": "Add the bot token first." }));
    }
    let adapter = TelegramAdapter::new(effective.clone());
    let bot = match bounded_telegram(adapter.connected_as()).await {
        Ok(bot) => bot,
        Err(error) => return Ok(serde_json::json!({ "ok": false, "error": error })),
    };
    let changed = !requested.is_empty() && sec.telegram.bot_token != effective;
    if changed {
        sec.telegram.bot_token = effective;
        secrets::save(&secrets_path, &sec)?;
    }
    let settings_path = settings::path_in(&s.data_dir);
    let mut cfg = settings::load(&settings_path);
    if changed {
        // A different token invalidates the earlier answer about bot and channel.
        cfg.telegram_verified = None;
        cfg.telegram_verify_error.clear();
        cfg.telegram_chat_title.clear();
        settings::save(&settings_path, &cfg)?;
    }
    let chat_title = cfg
        .telegram_verified
        .as_ref()
        .map(|check| check.chat.clone())
        .unwrap_or_default();
    Ok(serde_json::json!({ "ok": true, "bot": bot, "chat_title": chat_title }))
}

/// Read-only list of every channel and group this bot can see.
#[tauri::command]
pub async fn telegram_discover(s: State<'_, Shared>) -> Result<serde_json::Value, String> {
    let token = saved_telegram_token(&s);
    if token.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "error": "Connect the bot first." }));
    }
    let adapter = TelegramAdapter::new(token);
    match bounded_telegram(adapter.discover()).await {
        Ok(chats) => Ok(serde_json::json!({
            "ok": true,
            "chats": chats.iter().map(telegram_chat_json).collect::<Vec<_>>(),
        })),
        Err(error) => Ok(serde_json::json!({ "ok": false, "error": error })),
    }
}

/// Read-only resolution, used when only the title needs checking.
#[tauri::command]
pub async fn telegram_resolve(
    s: State<'_, Shared>,
    text: String,
) -> Result<serde_json::Value, String> {
    let token = saved_telegram_token(&s);
    if token.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "error": "Connect the bot first." }));
    }
    let adapter = TelegramAdapter::new(token);
    match bounded_telegram(adapter.resolve(&text)).await {
        Ok(chat) => Ok(serde_json::json!({ "ok": true, "chat": telegram_chat_json(&chat) })),
        Err(error) => Ok(serde_json::json!({ "ok": false, "error": error })),
    }
}

/// Pick a discovered channel or paste a link: only a resolved answer is saved.
#[tauri::command]
pub async fn telegram_use_channel(
    s: State<'_, Shared>,
    text: String,
) -> Result<serde_json::Value, String> {
    let token = saved_telegram_token(&s);
    if token.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "error": "Connect the bot first." }));
    }
    let adapter = TelegramAdapter::new(token);
    let checked = bounded_telegram(async {
        let chat = adapter.resolve(&text).await?;
        let bot = adapter.connected_as().await?;
        let check = settings::TelegramCheck {
            bot,
            chat: chat.title.clone(),
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        };
        Ok((chat, check))
    })
    .await;
    let (chat, check) = match checked {
        Ok(answer) => answer,
        Err(error) => return Ok(serde_json::json!({ "ok": false, "error": error })),
    };
    let path = settings::path_in(&s.data_dir);
    let mut cfg = settings::load(&path);
    cfg.telegram_chat = chat.id.clone();
    cfg.telegram_chat_title = chat.title.clone();
    cfg.telegram_verified = Some(check);
    cfg.telegram_verify_error.clear();
    settings::save(&path, &cfg)?;
    Ok(serde_json::json!({ "ok": true, "chat": telegram_chat_json(&chat) }))
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
/// Gateway is where Wire's models come from. A blank field keeps what is saved, so pressing the
/// button twice with nothing typed asks Gateway again — one path for Connect and Check connection.
#[tauri::command]
pub async fn save_gateway(
    s: State<'_, Shared>,
    url: String,
    key: String,
) -> Result<serde_json::Value, String> {
    Ok(crate::gateway::connect(&s.data_dir, &url, &key).await)
}

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

/// Read-only catalogue from Media Studio's own store. Same helper as the headless route.
#[tauri::command]
pub fn media_products(
    s: State<'_, Shared>,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    Ok(crate::media::products_json(&s.data_dir, limit.unwrap_or(crate::media::DEFAULT_LIMIT)))
}

/// Save where Media's store lives (empty = discover again); answers like `media_products`.
#[tauri::command]
pub fn save_media_path(s: State<'_, Shared>, path: String) -> Result<serde_json::Value, String> {
    crate::media::save_path(&s.data_dir, &path)
}
