# Wire SIGTERM acceptance — signed .app, hidden, 2026-11-25

## What was run

- Branch: `work/wire-sigterm-20261125`
- App code commit: `473334782d4e10bedb0a36eaf70fd0d87068beab`
- Signed `.app`: `src-tauri/target/release/bundle/macos/Wire Studio KH.app`
  - `CFBundleShortVersionString` 0.5.4
  - `CFBundleIdentifier` `com.terranash.wirestudiokh`
  - `codesign --verify --deep --strict`: valid on disk, satisfies its Designated Requirement
  - Authority `KH Studio Local Signing`; TeamIdentifier not set; not notarized
  - Built with `npx tauri build --bundles app`, then Sparkle copied/signed and the app signed
    exactly as `scripts/build-release.sh` does; **no DMG was made and nothing was published**.
- Command: `KEEP_EVIDENCE=1 WIRE_APP="<the signed .app>" bash scripts/sigterm-proof.sh`
- stdout: `lead/sigterm-20261125/signed-app-sigterm.log`
- per-process logs: `lead/sigterm-20261125/evidence/` (home paths in copied logs are rewritten to `$HOME`)
- final queue dump: `lead/sigterm-20261125/evidence/queue-after.txt`

No window, no pointer, no keyboard: every process was launched with `--hidden` and
`WIRE_STUDIO_KH_HIDDEN=1` into an isolated `HOME`, `TMPDIR` and `WIRE_STUDIO_KH_DATA_DIR`.

## In-flight job

A real queued delivery was moved due, then a real hidden `run_app()` was started with
`HTTPS_PROXY` pointed at a local proxy. The proxy accepted the adapter's `CONNECT
api.telegram.org:443` and withheld its answer for 4 s before returning 502. No connection reached
Telegram, and no certificate was trusted.

Immediately before signalling, the queue row and hold said:

```text
[inflight-sigterm] Job in flight at signal time: state=sending attempts=1 proxy_connects=1
[inflight-sigterm] Update hold while job is in flight: {"dispatch_paused_s": 25, "in_flight": 1, "safe": false, "unsettled": ["c05ca308-7636-4acb-a953-7d6db6bda503"]}
```

`SIGTERM` then landed while that request was still inside the proxy:

```text
[inflight-sigterm] SIGTERM: exit_code=0; all_gone=True in 3.557s; survivors=[]; hold_port_free=True; foreground_unchanged=True; no_persisted_hold_file=True; job_after=('pending', 1, '2026-11-25T14:33:07Z', 'sendMessage: error sending request for url (https://api.telegram.org/bot[redacted]/sendMessage)')
[inflight-sigterm] Shutdown ended the in-flight job: state=pending outcome='sendMessage: error sending request for url (https://api.telegram.org/bot[redacted]/sendMessage)'
```

The 3.584 s exit is the shutdown path waiting for the in-flight request; the idle held run exits in
0.119 s. The app logged:

```text
shutdown: signal=SIGTERM held=true settled=true in_flight=0 unsettled=[] wal_checkpoint=ok
```

The job did not stay `sending`: it ended `pending`, attempts 1, with the adapter's named transport
error. `PRAGMA quick_check` was `ok`, no hold file persisted, no process survived, port 8649 was free
and the foreground application did not change.

## Restart

- Clean restart after the settled job: startup log contains no `queue startup: released` line, hold
  was safe with `in_flight=0`, exit 0 in 0.112 s. Nothing stale was found.
- Deliberately simulated stale `sending` row: restart logged
  `queue startup: released 1 interrupted sending delivery(ies) back to pending`, the row returned to
  `pending` attempts 1 claimed_at NULL, `quick_check=ok`.

## What this proves and what it does not

Proved: the packaged, signed app's real dispatch/adapter path can be in flight when SIGTERM lands;
the shutdown path waits for it (bounded by the 8 s settle ceiling), records a settled outcome, leaves
no `sending` row and no persisted hold, and the next start finds nothing stale after a clean settle.

Not proved here:

- A successful Telegram publish during shutdown. The local proxy returned 502, so the named outcome
  is a transport failure. This exercises the real adapter and dispatch code, not Telegram itself.
- Installation, DMG, feed, appcast, Sparkle update or public release. This is a local signed proof
  build only; 0.5.4 remains published and unchanged.
- A job that cannot settle inside the 8 s ceiling. The separate bounded-unsettled unit test covers
  that path; this run did not force it.
