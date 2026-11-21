//! The dispatcher: one timer to the next due delivery, no polling loop.
//! Claims one row at a time, publishes through the target's adapter, and
//! writes the terminal or retry state before claiming the next.

use std::sync::atomic::Ordering;
use std::time::Duration;

use chrono::Utc;

use crate::adapters::{
    classify, telegram::TelegramAdapter, FailureClass, PlatformAdapter, PublishPayload,
};
use crate::queue::Failure;
use crate::secrets;
use crate::Shared;

/// Delay before re-checking when the store has no next due row.
const IDLE_TICK: Duration = Duration::from_secs(30);

pub fn spawn(state: Shared) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("dispatch runtime");
        loop {
            if state.paused.load(Ordering::Relaxed) {
                // The pause is in-memory and expires by itself, even when no
                // request ever arrives to refresh it (an install that dies).
                let expired = state
                    .paused_until
                    .lock()
                    .ok()
                    .and_then(|guard| *guard)
                    .map(|at| Utc::now() >= at)
                    .unwrap_or(false);
                if expired {
                    state.paused.store(false, Ordering::Relaxed);
                    if let Ok(mut guard) = state.paused_until.lock() {
                        *guard = None;
                    }
                    eprintln!("queue resumed: hold expired");
                } else {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
            }

            let claimed = {
                let Ok(mut store) = state.store.lock() else {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                };
                match store.claim_due(Utc::now()) {
                    Ok(row) => row,
                    Err(e) => {
                        eprintln!("queue claim failed: {e}");
                        None
                    }
                }
            };

            let Some(claim) = claimed else {
                // Sleep until the next due row, capped so a hold or new post is seen promptly.
                let wait = {
                    let Ok(store) = state.store.lock() else {
                        std::thread::sleep(IDLE_TICK);
                        continue;
                    };
                    store
                        .next_due()
                        .ok()
                        .flatten()
                        .map(|at| {
                            let secs = (at - Utc::now()).num_seconds().max(0) as u64;
                            Duration::from_secs(secs.min(IDLE_TICK.as_secs()))
                        })
                        .unwrap_or(IDLE_TICK)
                };
                std::thread::sleep(wait);
                continue;
            };

            let (failure, reason) = match publish(&state, &rt, &claim) {
                Ok((remote_id, remote_url)) => {
                    let mut store = state.store.lock().expect("store");
                    if let Err(e) = store.mark_sent(
                        &claim.delivery_id,
                        remote_id.as_deref(),
                        remote_url.as_deref(),
                    ) {
                        eprintln!("queue settle failed: {e}");
                    }
                    continue;
                }
                Err(err) => (err.classify(), err.message.clone()),
            };

            let mut store = state.store.lock().expect("store");
            if let Err(e) = store.mark_failed(&claim.delivery_id, failure, &reason) {
                eprintln!("queue failure settle failed: {e}");
            }
        }
    });
}

struct DispatchError {
    message: String,
    failure: Failure,
}

impl DispatchError {
    fn classify(&self) -> Failure {
        self.failure
    }
}

/// Publishes one claimed delivery. 0.1.0 has one platform (Telegram).
fn publish(
    state: &Shared,
    rt: &tokio::runtime::Runtime,
    claim: &crate::queue::Claim,
) -> Result<(Option<String>, Option<String>), DispatchError> {
    let (platform, chat_id) = match claim.target.split_once(':') {
        Some((p, rest)) => (p, rest.to_string()),
        None => (
            "telegram",
            claim.target.trim_start_matches("telegram").trim_start_matches(':').to_string(),
        ),
    };

    if platform != "telegram" {
        return Err(DispatchError {
            message: format!("{platform} is not connected in 0.1.0"),
            failure: Failure::Permanent,
        });
    }

    let secrets = secrets::load(&secrets::path_in(&state.data_dir));
    if secrets.telegram.bot_token.is_empty() {
        return Err(DispatchError {
            message: "No Telegram bot token yet — add one in Targets".into(),
            failure: Failure::AuthPermission,
        });
    }

    let adapter = TelegramAdapter::new(secrets.telegram.bot_token.clone());
    let payload = PublishPayload {
        target: chat_id,
        text: claim.text.clone(),
        media: None,
        dry_run: false,
    };

    let outcome = rt.block_on(adapter.publish(payload));
    match outcome {
        Ok(ok) => Ok((ok.remote_id, ok.remote_url)),
        Err(e) => {
            let failure = match classify(&e) {
                FailureClass::Permanent => Failure::Permanent,
                FailureClass::AuthPermission => Failure::AuthPermission,
                FailureClass::Transient => Failure::Transient,
            };
            Err(DispatchError {
                message: e.message,
                failure,
            })
        }
    }
}
