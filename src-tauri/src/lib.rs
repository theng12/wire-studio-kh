//! Wire Studio KH core: queue, delivery adapters, Gateway client, update hold.

pub mod adapters;
pub mod gateway;
pub mod hold;
pub mod queue;
pub mod secrets;
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

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .manage(state)
        .run(tauri::generate_context!())
        .expect("error while running Wire Studio KH");
}
