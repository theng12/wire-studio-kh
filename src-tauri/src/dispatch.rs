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
            if state.shutting_down.load(Ordering::SeqCst) {
                break;
            }
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
                    if let Ok(guard) = state.wake_lock.lock() {
                        let _ = state.wake.wait_timeout(guard, Duration::from_millis(200));
                    }
                    continue;
                }
            }

            if state.shutting_down.load(Ordering::SeqCst) {
                break;
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
                // A queue change wakes this at once instead of waiting the tick out.
                if let Ok(guard) = state.wake_lock.lock() {
                    let _ = state.wake.wait_timeout(guard, wait);
                }
                continue;
            };

            state.in_flight.fetch_add(1, Ordering::SeqCst);
            let outcome = publish(&state, &rt, &claim);
            match outcome {
                Ok((remote_id, remote_url)) => {
                    let mut store = state.store.lock().expect("store");
                    if let Err(e) = store.mark_sent(
                        &claim.delivery_id,
                        remote_id.as_deref(),
                        remote_url.as_deref(),
                    ) {
                        eprintln!("queue settle failed: {e}");
                    }
                }
                Err(err) => {
                    let mut store = state.store.lock().expect("store");
                    if let Err(e) = store.mark_failed(&claim.delivery_id, err.classify(), &err.message) {
                        eprintln!("queue failure settle failed: {e}");
                    }
                }
            }
            state.in_flight.fetch_sub(1, Ordering::SeqCst);
            state.wake();
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

/// The text actually sent: the caption, plus the product line built from a live read of Media.
/// One function for the dispatch path and the headless preview, so they cannot drift.
/// Returns (text, the price read now, the moment it was read — RFC3339 in Phnom Penh time).
pub fn final_text(state: &Shared, claim: &crate::queue::Claim) -> (String, Option<f64>, Option<String>) {
    let Some(id) = claim.product_id.as_deref() else {
        return (claim.text.clone(), claim.price_shown, None);
    };
    let now = chrono::Utc::now();
    let as_of = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let clock = now
        .with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).expect("UTC+7"))
        .format("%H:%M")
        .to_string();
    match crate::media::product(&state.data_dir, id) {
        Ok(product) => {
            let line = match product.price_retail {
                // No currency symbol: Media records no currency, and a guessed one would be a lie.
                Some(price) => format!("{} · {} (as of {})", product.name, price, clock),
                None => format!("{} · price unavailable (as of {})", product.name, clock),
            };
            (format!("{}\n\n{line}", claim.text), product.price_retail, Some(as_of))
        }
        Err(reason) => {
            let line = format!("price unavailable — {reason} (as of {clock})");
            (format!("{}\n\n{line}", claim.text), None, Some(as_of))
        }
    }
}

/// Publishes one claimed delivery. 0.1.0 has one platform (Telegram).
fn publish(
    state: &Shared,
    rt: &tokio::runtime::Runtime,
    claim: &crate::queue::Claim,
) -> Result<(Option<String>, Option<String>), DispatchError> {
    // Read the price and record what this attempt sends *before* any refusal: the row is a record
    // of what the send path saw, not only of a successful publish.
    let (text, price, as_of) = final_text(state, claim);
    if let Some(stamp) = as_of.as_deref() {
        if let Ok(mut store) = state.store.lock() {
            let _ = store.record_sent_price(&claim.delivery_id, price, stamp);
        }
    }

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
        text: text.clone(),
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
