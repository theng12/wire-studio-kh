# Wire Studio KH — working rules

Plan: `_orchestra/youtube-stack/wire-studio-scope/PLAN-V2.md` (approved 2026-11-21).

- Tauri 2: Rust core owns queue, adapters, Gateway client, data and the update hold;
  the web UI (`ui/`) owns screens.
- No native confirm/alert/prompt (silent in a packaged webview) — in-app dialogs only.
- No Keychain. Secrets in one `0600` file in the data dir, masked in UI and logs.
- Update hold: `POST/DELETE /api/maintenance/hold` on `127.0.0.1:8649`, loopback only,
  Launcher `hold-token` when present, fail-open with one log line when absent.
- Headless entry: `--test-mode` with `KH_TEST_PORT`, temp data dir, no window.
- Every UI action has a guarded programmatic equivalent; the headless surface is those routes.
- Store UTC, show UTC+7. Honest states: "not checked" is never "passed", "no data" never 0.
- Tests never take the owner's desktop. Commit as `112568204+theng12@users.noreply.github.com`.
