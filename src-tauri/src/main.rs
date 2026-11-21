fn main() {
    let args: Vec<String> = std::env::args().collect();
    let test_mode = args.iter().any(|a| a == "--test-mode");
    let port = args
        .iter()
        .position(|a| a == "--KH_TEST_PORT")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse::<u16>().ok())
        .or_else(|| std::env::var("KH_TEST_PORT").ok().and_then(|p| p.parse().ok()));

    if test_mode {
        // A temp data dir per run unless the caller set one (scripts share it across a restart).
        let dir = match std::env::var("WIRE_STUDIO_KH_DATA_DIR") {
            Ok(dir) if !dir.is_empty() => std::path::PathBuf::from(dir),
            _ => {
                let dir = std::env::temp_dir()
                    .join(format!("wire-studio-kh-test-{}", std::process::id()));
                std::env::set_var("WIRE_STUDIO_KH_DATA_DIR", &dir);
                dir
            }
        };
        let state = wire_studio_kh::init_state(&dir).expect("test state");
        let p = port.unwrap_or(18765);

        // SIGTERM ends the run cleanly, so a harness sees exit 0 and the port free.
        extern "C" fn on_term(_sig: libc::c_int) {
            std::process::exit(0);
        }
        unsafe {
            libc::signal(libc::SIGTERM, on_term as libc::sighandler_t);
            libc::signal(libc::SIGINT, on_term as libc::sighandler_t);
        }

        wire_studio_kh::start_hold(state.clone(), 8649);
        wire_studio_kh::dispatch::spawn(state.clone());
        wire_studio_kh::testmode::serve(state, p).expect("test-mode server");
        return;
    }

    wire_studio_kh::run_app();
}
