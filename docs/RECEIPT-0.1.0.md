# Wire Studio KH 0.1.0 — build receipt (2026-11-21)

**State: built and tested, not yet packaged or published.** No DMG, no feed entry,
no install. Commit `1bff642` on `main` of the new repo
`~/Developer/My Own Apps/wire-studio-kh`.

## What runs

- `cargo build` (dev profile) → `src-tauri/target/debug/wire-studio-kh`, 48.7 MB.
- Headless entry: `wire-studio-kh --test-mode --KH_TEST_PORT <n>` → prints
  `TEST_URL`, `TEST_TOKEN`, `TEST_DATA_DIR`; temp data dir per run; SIGTERM exits 0
  and frees the port; never connects to WindowServer (no window can exist).
- Update hold: `POST/DELETE /api/maintenance/hold` on `127.0.0.1:8649`, loopback,
  foreign Origin refused, Launcher `hold-token` honoured when present, fail-open with
  one logged line when absent, pause expires by itself, dispatcher wakes on release.
- Queue: one delivery row per (post,target), atomic claim, 5-attempt ceiling with
  1/2/4/8/16 min backoff, auth/permission → needs-attention, catch-up lateness
  recorded, cancel/retry terminal rules, UTC stored / UTC+7 shown.
- Telegram adapter (sendMessage, status mapping, Retry-After, token scrubbing);
  YouTube and Facebook adapter skeletons behind the same trait, refusing honestly.
- Secrets: one `app-secrets.json` 0600, atomic write, masked, redacted logs, no
  Keychain. Settings versioned in `settings.json`.
- Feed check: reads `~/Library/KHUpdater/publisher.json`, verifies Ed25519 over the
  exact bytes, compares versions, five honest states.
- UI shell: sidebar + footer group, Composer, Queue (7 states, retry/cancel with
  in-app confirm), Targets, Settings (Appearance System/Light/Dark), What's New,
  in-app dialogs only, keyboard set, Paper ground with `#A6402C` accent.

## Evidence

| Check | Result |
|---|---|
| `cargo test --lib` | **36 passed, 0 failed** |
| `scripts/test-mode-smoke.sh` | **12/12 PASS** |
| `scripts/hold-proof.sh` (live queue, kill -9, restart, DELETE, 2 s expiry) | **25/25 PASS** |
| native dialogs scan (`ui/`) | PASS, none |
| Keychain scan (`src-tauri/src`) | PASS, none |
| desktop-safety scan (`scripts/`) | PASS — nothing takes the desktop |
| CHANGELOG heading matches VERSION | PASS |

## Not done / not checked (honest)

- **No DMG, no signing step, no publish.** Bundle is `active:false`; the feed id
  `wire` is not yet allowlisted by the Updater side, and publishing needs the
  publisher host run. Nothing has been read back from the 1301 feed because nothing
  was published.
- **No live Telegram post.** No bot token exists on this Mac (checked sociobot's
  config, Story's secret store, bounded token-pattern scan). The owner offered one
  line; until then dispatch parks posts at "needs attention: add a Telegram bot
  token", which is the honest state.
- **Idle footprint: measured 60 s only, not the 10-minute assert.** 10-minute run
  with timer-wake counting is still to do.
- **UI verified headlessly against mocked state only** (the UI agent's browser
  checks). The window was never opened on the owner's desktop, so the shipped-app
  click-through is not yet proven.
- **Instagram shelved** per the owner; nothing public built.
- Story Still Posts import is 0.2.0, not started; Story keeps its copy until Wire's
  own capability ships and imports are proven.

## Open for the owner

One line: a Telegram bot token (BotFather) and the channel it administers. Everything
else proceeds without it.
