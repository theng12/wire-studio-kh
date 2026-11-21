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
        let dir = std::env::temp_dir().join(format!("wire-studio-kh-test-{}", std::process::id()));
        let state = wire_studio_kh::init_state(&dir).expect("test state");
        let p = port.unwrap_or(18765);
        wire_studio_kh::start_hold(state.clone(), 8649);
        wire_studio_kh::testmode::serve(state, p).expect("test-mode server");
        return;
    }

    wire_studio_kh::run_app();
}
