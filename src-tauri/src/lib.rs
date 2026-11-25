//! Wire Studio KH core: queue, delivery adapters, Gateway client, update hold.

pub mod adapters;
pub mod commands;
pub mod dispatch;
pub mod feedcheck;
pub mod formats;
pub mod gateway;
pub mod hold;
pub mod import;
pub mod media;
pub mod queue;
pub mod secrets;
pub mod settings;
pub mod shutdown;
pub mod testmode;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use queue::Store;

/// Shared core state. One instance per process.
pub struct AppState {
    pub store: Mutex<Store>,
    pub paused: AtomicBool,
    pub paused_until: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    pub data_dir: PathBuf,
    pub in_flight: std::sync::atomic::AtomicUsize,
    /// Set once SIGTERM/SIGINT begins graceful teardown; stops dispatch claiming new work.
    pub shutting_down: AtomicBool,
    /// The dispatcher sleeps here until the next due row or a queue change.
    pub wake: std::sync::Condvar,
    pub wake_lock: Mutex<()>,
}

impl AppState {
    /// Re-arms the dispatcher: called by every path that adds or changes work.
    pub fn wake(&self) {
        let _guard = self.wake_lock.lock();
        self.wake.notify_all();
    }
}

pub type Shared = Arc<AppState>;

/// Data dir: ~/Library/Application Support/Wire Studio KH, or the test override.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("WIRE_STUDIO_KH_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join("Library/Application Support")
        .join("Wire Studio KH")
}

pub fn init_state(dir: &Path) -> Result<Shared, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create data dir {}: {e}", dir.display()))?;
    let mut store = Store::open(&dir.join("wire.db")).map_err(|e| format!("cannot open queue: {e}"))?;
    match store.release_stale_sending(0) {
        Ok(0) => {}
        Ok(n) => eprintln!("queue startup: released {n} interrupted sending delivery(ies) back to pending"),
        Err(e) => eprintln!("queue startup: stale-sending reclaim failed: {e}"),
    }
    Ok(Arc::new(AppState {
        store: Mutex::new(store),
        paused: AtomicBool::new(false),
        paused_until: Mutex::new(None),
        data_dir: dir.to_path_buf(),
        in_flight: std::sync::atomic::AtomicUsize::new(0),
        shutting_down: AtomicBool::new(false),
        wake: std::sync::Condvar::new(),
        wake_lock: Mutex::new(()),
    }))
}

/// Hidden desktop launch: runs the real packaged Tauri event loop, hold server, and Sparkle shim
/// with zero onscreen windows and zero focus change (used by isolated SIGTERM acceptance).
pub fn launch_hidden() -> bool {
    std::env::args().any(|a| a == "--hidden")
        || std::env::var("WIRE_STUDIO_KH_HIDDEN").ok().as_deref() == Some("1")
}

/// Starts the hold listener on the fixed loopback port.
pub fn start_hold(state: Shared, port: u16) {
    std::thread::spawn(move || {
        if let Err(e) = hold::serve(state, port) {
            // Fail open: the app still runs and still posts; it just cannot be held.
            eprintln!("hold unavailable on 127.0.0.1:{port}: {e}");
        }
    });
}

/// Sparkle through the shared shim (INTEGRATION.md §2). The shim owns the hold-then-check order and
/// the no-UI driver; this file only starts it and logs a refusal with its reason.
#[cfg(target_os = "macos")]
mod sparkle {
    extern "C" {
        fn kh_sparkle_start() -> i32;
        fn kh_sparkle_last_error() -> *const std::ffi::c_char;
        fn kh_sparkle_set_state_callback(cb: extern "C" fn(i32));
    }

    extern "C" fn on_state(state: i32) {
        // 2 = relaunching: the shim is about to terminate the app for the swap.
        if state == 2 {
            eprintln!("kh-sparkle: relaunching for an update");
        }
    }

    pub fn start() {
        unsafe {
            kh_sparkle_set_state_callback(on_state);
            let rc = kh_sparkle_start();
            if rc != 0 {
                let reason = std::ffi::CStr::from_ptr(kh_sparkle_last_error()).to_string_lossy();
                eprintln!("kh-sparkle: refused: {reason}");
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod sparkle {
    pub fn start() {}
}

/// The desktop app. One window, reopened from the app icon and the menu.
pub fn run_app() {
    let hidden = launch_hidden();
    let state = init_state(&data_dir()).expect("core state");
    let shutdown_state = state.clone();
    start_hold(state.clone(), 8649);
    dispatch::spawn(state.clone());

    let mut app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(move |app, _args, _cwd| {
            if !hidden {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        }))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::state,
            commands::queue_post,
            commands::cancel_delivery,
            commands::retry_delivery,
            commands::save_telegram,
            commands::telegram_connected_as,
            commands::telegram_discover,
            commands::telegram_resolve,
            commands::telegram_use_channel,
            commands::save_gateway,
            commands::save_appearance,
            commands::check_updates,
            commands::story_import_preview,
            commands::story_import_run,
            commands::list_dirs,
            commands::media_products,
            commands::save_media_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Wire Studio KH");

    if hidden {
        #[cfg(target_os = "macos")]
        app.set_activation_policy(tauri::ActivationPolicy::Prohibited);
    } else {
        use tauri::Manager;
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
        }
    }

    // Install graceful SIGTERM/SIGINT before Sparkle starts: settles in-flight work, checkpoints
    // SQLite, logs the outcome, and quits through Tauri's own path with a bounded 2 s fallback.
    let exit_handle = app.handle().clone();
    shutdown::install(shutdown_state, move || {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(2));
            eprintln!("shutdown: fallback exit after Tauri quit timeout");
            std::process::exit(0);
        });
        exit_handle.exit(0);
    });

    // After the window exists, before anything else starts: the shim takes the hold first.
    sparkle::start();

    app.run(move |handle, event| {
        use tauri::Manager;
        if let tauri::RunEvent::Reopen { .. } = event {
            if !hidden {
                if let Some(w) = handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        }
    });
}
