# Wire Studio KH 0.4.3 — the Gateway connection (2026-11-22)

Shipped: a real Wire ↔ Gateway connection, exercised against a real local Gateway before
anything was written, plus the folder-browser proof and the last "core" wording.

## Fault by fault (each was met during the exercise, then fixed)

| # | Fault met | Cause | Fixed in 0.4.3 |
|---|---|---|---|
| W1 | Wire could not connect to Gateway at all | `gateway.rs` had no caller — no command, no UI, no test route | Settings → Gateway card; one shared `gateway::connect` behind the window, the headless route and the state block |
| W2 | A wrong key looked healthy | `/health` answers 200 to any bearer; only `/v1/models` answers 401/403 | verification reads `/v1/models` only; Gateway's own sentence is shown ("Invalid API key.") |
| W3 | An empty catalogue could read as failure or as a silent zero | a Gateway with no providers publishes `{"models":[]}` | "connected · no models published yet", never an error and never a bare 0 |
| W4 | A test Gateway rewrote `~/.kh/gateway-endpoint.json` (Relay's pointer) | the probe did not pin `GATEWAY_ENDPOINT_FILE` | pinned into the proof's temp dir; the proof records the live file's sha256 before and after and fails if it changed; issue row 20261122-315 |
| W5 | Choose… had no headless proof | `list_dirs` was reachable only from a window | `/api/test/trigger/list-dirs` + two asserts (real folders listed, unreadable folder says why) |
| W6 | "core" still on screen in seven strings | the 0.4.2 rule was applied only partly | all seven fixed; a repeat grep is now a gate step (`no-core-on-screen`) |
| W7 | `Cargo.lock` trails a version bump | lockfile updated by hand in 0.4.2 only | updated in the 0.4.3 commit |

## Gate

- Command: `zsh _orchestra/tools/gate-run.sh _orchestra/tools/gate-configs/wire.sh`
- Verdict: **GATE PASS wire 3f28302294bc1de84e8d4999637ca987408d09fc 2026-11-22T03:07:27Z**, exit 0.
- Steps: `version-file`, `cargo-version`, `tauri-version`, `changelog-heading`, `unit-tests`
  (67 passed, 0 failed, 1 ignored), `no-native-dialogs`, `no-keychain`, `no-core-on-screen`,
  `test-mode-smoke`, `hold-proof`, `gateway-connect` (21 asserts), `import-route`, plus the runner's
  `desktop-safety` and `commit-identity`. Tree `dirty=0` at both identity reads.
- Deliberately failing probe in the same wrapper run: `GATE FAILED fail_probe (rc=1)`.
- Harness: `gate-run.sh` sha256 `2d5de68a9a305db567db356d2f63c6e4cc22caf6397a0c93b02cc8518889e4c6`
  identical before and after — the verdict stands (a change would have written `GATE VOID`).
- `idle-footprint`: **UNMEASURED** — no running instance was handed to the gate. Separately
  measured: the headless core idles at 13 MB RSS and 0 % CPU, flat over a minute; the windowed
  app's idle footprint is **not measured**.
- Evidence: `_orchestra/developer-media/wire-studio-kh/lead/gate-20261122-100646-15632/`.

## What the connect proof actually does

`scripts/gateway-connect-proof.sh` boots a **real Gateway** (from `_graphs/gateway`, its own temp
`GATEWAY_APP_SUPPORT`/`GATEWAY_DATA`/`GATEWAY_ENV_FILE`, pinned `GATEWAY_ENDPOINT_FILE`) and runs
Wire hidden in test mode with a temp data dir:

- a wrong key is refused with Gateway's own sentence; the right key connects
- an empty catalogue reads "no models published yet"
- the key never appears in state, log or answer; only `•••• tail` is shown; it lives in the 0600
  secrets file, not `settings.json`
- rotating the key in Gateway's `.env` (a live revocation — Gateway re-reads it on change) turns the
  saved connection into `failed`; reconnecting with the new key works
- a Gateway that is not running says "Nothing is listening at http://…"
- Choose… lists real folders; an unreadable folder says why
- the live `~/.kh/gateway-endpoint.json` is byte-identical afterwards
- no window, no WindowServer connection, SIGTERM exits 0

21 PASS, 0 FAIL.

## Published artifact

| | |
|---|---|
| DMG | `Wire Studio KH_0.4.3_aarch64.dmg`, **4,579,444 bytes** |
| sha256 | `34b422723b14586acb02585b5aa58a1635b380a4a829483532500be23831001c` |
| Built from | `3f28302` (the gated head), 2026-11-22 12:14 |
| Feed file | `34b422723b14-Wire Studio KH_0.4.3_aarch64.dmg` |
| Publisher | `~/Library/KHUpdater/base/current` → `base/versions/0.1.79/khu`, 2026-11-22 11:04 |
| Public scan | `scan-public.sh` on the published bytes: `RESULT hits=0`, PASS |

### Served-feed readback from 1301 (not from the script)

```
version      0.4.3
type         media
sha256       34b422723b14586acb02585b5aa58a1635b380a4a829483532500be23831001c
bytes        4579444
file         34b422723b14-Wire Studio KH_0.4.3_aarch64.dmg
tagline      Posts one caption to your social accounts.
description  Composes one caption and sends it to your social accounts, each named by you
             rather than a technical key, with a Gateway connection you can check at any time.
holdURL      http://127.0.0.1:8649/api/maintenance/hold
holdSince    0.1.0
released     2026-11-22T05:21:36.304Z
```

Tagline and description match `FEED-TEXT-APPROVED-20261122.md` word for word (the file's "41" is a
miscount; the served string is 42 characters and well under 62). The hold pair is unchanged because no
hold flag was passed. `feed-verify.sh wire --expect-version 0.4.3 --expect-sha 34b4227…` → PASS.

## Retention

`dist/` now holds 0.4.3 (current) + 0.4.2 (previous). 0.4.1 retired recoverably to
`~/Desktop/_trash/wire-studio-kh/2026-11-22 0.4.1 dmg/` with `MANIFEST.md` and its sha256; nothing
referenced it.

## Not done / not checked

- **The Settings → Gateway card was not clicked in a window.** This install has no in-app Browser
  preview, and a Playwright window would take the owner's desktop. The command behind the button
  (`save_gateway` → `gateway::connect`) is proven live against a real Gateway; the click itself is
  the shipped-app acceptance step.
- **Packaged-app SIGTERM with queued work and an active hold**: deferred. One isolated run stopped
  67 ms after SIGTERM; the full run tripped the desktop-safety check (`open -g -j` still produced an
  on-screen window) and was stopped. Not verified: descendant exit, quit-time writes. Row 20261122-315.
- **Windowed idle footprint**: not measured (gate reports UNMEASURED).
- YouTube and Facebook connections still ship later; their destinations stay unselectable for a text
  post, which is what the screen says.

## Post-release commit

The 0.4.3 DMG was built and published from `3f28302`, the gated head. `scripts/publish.sh` (publisher
path + flag guard + the approved feed words) and `ABOUT.md` landed in **one** later commit, `e221da5`,
which ships nothing; the next ordinary release's gate covers it.

*Wire Studio - Lead - 002, 2026-11-22.*
