//! Graceful SIGTERM/SIGINT handling for both desktop (`run_app`) and headless (`--test-mode`).
//!
//! Uses a POSIX self-pipe so the signal handler runs only async-signal-safe operations and the
//! watcher thread blocks with zero polling when idle. On signal: closes dispatch, waits bounded
//! for any in-flight delivery to settle, checkpoints SQLite WAL, logs the outcome, and invokes
//! the caller's exit callback.

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use crate::{hold, Shared};

/// Maximum time to wait for an in-flight publish to settle on SIGTERM before exiting anyway.
/// Kept strictly under the 10 s SIGTERM exit contract so teardown + fallback still finish in time.
pub const SETTLE_CEILING: Duration = Duration::from_secs(8);

static SIGNAL_WRITE_FD: AtomicI32 = AtomicI32::new(-1);
static RECEIVED_SIGNAL: AtomicI32 = AtomicI32::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownOutcome {
    pub signal: String,
    pub held_before: bool,
    pub settled: bool,
    pub in_flight: usize,
    pub unsettled: Vec<String>,
    pub wal_checkpoint_ok: bool,
    pub elapsed_ms: u64,
}

fn signal_name(sig: libc::c_int) -> String {
    match sig {
        libc::SIGTERM => "SIGTERM".into(),
        libc::SIGINT => "SIGINT".into(),
        other => format!("SIG({other})"),
    }
}

extern "C" fn on_signal(sig: libc::c_int) {
    let _ = RECEIVED_SIGNAL.compare_exchange(0, sig, Ordering::SeqCst, Ordering::SeqCst);
    let fd = SIGNAL_WRITE_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        let byte = [(sig & 0xff) as u8];
        unsafe {
            let _ = libc::write(fd, byte.as_ptr().cast(), 1);
        }
    }
}

/// Closes dispatch, waits up to `max_wait` for in-flight rows to settle, checkpoints WAL, and logs
/// one explicit outcome line. Never silent and never unbounded.
pub fn graceful_shutdown(state: &Shared, signal_name: &str, max_wait: Duration) -> ShutdownOutcome {
    let started = Instant::now();
    let held_before = state.paused.load(Ordering::SeqCst);

    state.shutting_down.store(true, Ordering::SeqCst);
    state.paused.store(true, Ordering::SeqCst);
    state.wake();

    let deadline = started + max_wait;
    let (count, ids, settled) = loop {
        let ids = hold::sending_ids(state);
        let count = hold::in_flight(state);
        if count == 0 && ids.is_empty() {
            break (count, ids, true);
        }
        let now = Instant::now();
        if now >= deadline {
            break (count, ids, false);
        }
        let step = (deadline - now).min(Duration::from_millis(50));
        if let Ok(guard) = state.wake_lock.lock() {
            let _ = state.wake.wait_timeout(guard, step);
        } else {
            std::thread::sleep(step);
        }
    };

    let wal_checkpoint_ok = match state.store.lock() {
        Ok(store) => match store.checkpoint() {
            Ok(_) => true,
            Err(error) => {
                eprintln!("shutdown: wal checkpoint failed: {error}");
                false
            }
        },
        Err(_) => {
            eprintln!("shutdown: store lock poisoned during wal checkpoint");
            false
        }
    };

    let elapsed_ms = started.elapsed().as_millis() as u64;
    eprintln!(
        "shutdown: signal={signal_name} held={held_before} settled={settled} in_flight={count} unsettled={ids:?} wal_checkpoint={} elapsed_ms={elapsed_ms}",
        if wal_checkpoint_ok { "ok" } else { "error" }
    );

    ShutdownOutcome {
        signal: signal_name.to_string(),
        held_before,
        settled,
        in_flight: count,
        unsettled: ids,
        wal_checkpoint_ok,
        elapsed_ms,
    }
}

/// Installs async-signal-safe SIGTERM/SIGINT handlers backed by a blocking self-pipe thread.
/// Calls `on_exit` once graceful shutdown finishes.
pub fn install(state: Shared, on_exit: impl FnOnce() + Send + 'static) {
    let mut fds = [-1 as libc::c_int; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        eprintln!("shutdown: pipe creation failed; SIGTERM handler not installed");
        return;
    }
    let (read_fd, write_fd) = (fds[0], fds[1]);
    unsafe {
        let _ = libc::fcntl(read_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        let _ = libc::fcntl(write_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        let flags = libc::fcntl(write_fd, libc::F_GETFL);
        if flags >= 0 {
            let _ = libc::fcntl(write_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }

    SIGNAL_WRITE_FD.store(write_fd, Ordering::SeqCst);
    RECEIVED_SIGNAL.store(0, Ordering::SeqCst);

    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_signal as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut());
    }

    let _ = std::thread::Builder::new()
        .name("wire-sigterm".into())
        .spawn(move || {
            let mut buf = [0u8; 1];
            loop {
                let n = unsafe { libc::read(read_fd, buf.as_mut_ptr().cast(), 1) };
                if n > 0 || RECEIVED_SIGNAL.load(Ordering::SeqCst) != 0 {
                    break;
                }
                if n == 0 {
                    break;
                }
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break;
            }
            unsafe {
                libc::close(read_fd);
            }
            let sig = RECEIVED_SIGNAL.load(Ordering::SeqCst);
            let name = if sig != 0 {
                signal_name(sig)
            } else {
                "SIGTERM".into()
            };
            let _ = graceful_shutdown(&state, &name, SETTLE_CEILING);
            on_exit();
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_state;
    use chrono::Utc;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wire-shutdown-test-{label}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn graceful_shutdown_idle_settles_immediately_and_preserves_queue() {
        let dir = temp_dir("idle");
        let state = init_state(&dir).expect("state");
        let id = {
            let mut store = state.store.lock().unwrap();
            let (post_id, ids) = store
                .create_post("queued before sigterm", &["telegram:TESTCHAT".into()])
                .unwrap();
            store
                .set_next_attempt_for_post(&post_id, Utc::now() + chrono::Duration::hours(1))
                .unwrap();
            ids[0].clone()
        };

        let out = graceful_shutdown(&state, "SIGTERM", Duration::from_secs(2));
        assert_eq!(out.signal, "SIGTERM");
        assert!(!out.held_before);
        assert!(out.settled);
        assert_eq!(out.in_flight, 0);
        assert!(out.unsettled.is_empty());
        assert!(out.wal_checkpoint_ok);
        assert!(out.elapsed_ms < 500, "idle shutdown took {}ms", out.elapsed_ms);
        assert!(state.shutting_down.load(Ordering::SeqCst));
        assert!(state.paused.load(Ordering::SeqCst));

        let rows = state.store.lock().unwrap().queue_view().unwrap();
        let row = rows.into_iter().find(|r| r.id == id).expect("row");
        assert_eq!(row.state, "pending");
        assert_eq!(row.attempts, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn graceful_shutdown_under_hold_records_hold_and_keeps_paused() {
        let dir = temp_dir("held");
        let state = init_state(&dir).expect("state");
        state.paused.store(true, Ordering::SeqCst);

        let out = graceful_shutdown(&state, "SIGTERM", Duration::from_secs(2));
        assert!(out.held_before);
        assert!(out.settled);
        assert_eq!(out.in_flight, 0);
        assert!(out.wal_checkpoint_ok);
        assert!(state.paused.load(Ordering::SeqCst));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn graceful_shutdown_waits_for_in_flight_to_settle() {
        let dir = temp_dir("settle");
        let state = init_state(&dir).expect("state");
        let id = {
            let mut store = state.store.lock().unwrap();
            let (_, ids) = store
                .create_post("in-flight send", &["telegram:TESTCHAT".into()])
                .unwrap();
            let claim = store.claim_due(Utc::now()).unwrap().expect("claimed");
            assert_eq!(claim.delivery_id, ids[0]);
            ids[0].clone()
        };
        state.in_flight.store(1, Ordering::SeqCst);

        let bg = state.clone();
        let bg_id = id.clone();
        let finisher = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            bg.store
                .lock()
                .unwrap()
                .mark_sent(&bg_id, Some("m1"), None)
                .unwrap();
            bg.in_flight.store(0, Ordering::SeqCst);
            bg.wake();
        });

        let out = graceful_shutdown(&state, "SIGTERM", Duration::from_secs(2));
        finisher.join().unwrap();

        assert!(out.settled);
        assert_eq!(out.in_flight, 0);
        assert!(out.unsettled.is_empty());
        assert!(out.elapsed_ms >= 100 && out.elapsed_ms < 1500, "elapsed {}ms", out.elapsed_ms);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn graceful_shutdown_bounds_unsettled_and_startup_reclaims_it() {
        let dir = temp_dir("unsettled");
        let state = init_state(&dir).expect("state");
        let id = {
            let mut store = state.store.lock().unwrap();
            let (_, ids) = store
                .create_post("stuck send", &["telegram:TESTCHAT".into()])
                .unwrap();
            let claim = store.claim_due(Utc::now()).unwrap().expect("claimed");
            assert_eq!(claim.delivery_id, ids[0]);
            ids[0].clone()
        };

        let out = graceful_shutdown(&state, "SIGTERM", Duration::from_millis(180));
        assert!(!out.settled);
        assert_eq!(out.in_flight, 1);
        assert_eq!(out.unsettled, vec![id.clone()]);
        assert!(out.wal_checkpoint_ok);
        assert!(out.elapsed_ms >= 150 && out.elapsed_ms < 1500, "elapsed {}ms", out.elapsed_ms);
        drop(state);

        // Next startup must reclaim the interrupted sending row back to pending.
        let restarted = init_state(&dir).expect("restarted state");
        let store = restarted.store.lock().unwrap();
        assert_eq!(store.in_flight_count().unwrap(), 0);
        let row = store
            .queue_view()
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .expect("row");
        assert_eq!(row.state, "pending");
        assert_eq!(row.attempts, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
