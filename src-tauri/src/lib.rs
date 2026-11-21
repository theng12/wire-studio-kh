//! Wire Studio KH core: queue, delivery adapters, Gateway client, update hold.

pub mod adapters;
pub mod commands;
pub mod dispatch;
pub mod feedcheck;
pub mod formats;
pub mod gateway;
pub mod hold;
pub mod import;
pub mod queue;
pub mod secrets;
pub mod settings;
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
    let store = Store::open(&dir.join("wire.db")).map_err(|e| format!("cannot open queue: {e}"))?;
    Ok(Arc::new(AppState {
        store: Mutex::new(store),
        paused: AtomicBool::new(false),
        paused_until: Mutex::new(None),
        data_dir: dir.to_path_buf(),
        in_flight: std::sync::atomic::AtomicUsize::new(0),
        wake: std::sync::Condvar::new(),
        wake_lock: Mutex::new(()),
    }))
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

/// The desktop app. One window, reopened from the app icon and the menu.
pub fn run_app() {
    let state = init_state(&data_dir()).expect("core state");
    start_hold(state.clone(), 8649);
    dispatch::spawn(state.clone());

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building Wire Studio KH");

    app.run(|handle, event| {
        use tauri::Manager;
        if let tauri::RunEvent::Reopen { .. } = event {
            if let Some(w) = handle.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    });
}
