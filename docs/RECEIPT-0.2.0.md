# Wire Studio KH 0.2.0 — first slice receipt (2026-11-21)

**State: import core done and tested; no release.** 0.2.0 as a batch is not finished —
the pool builder, recipes editor, renderer and the real Production import are still ahead.

## What is done

- `docs/STILL-POSTS-IMPORT-SPEC.md` — the exact Story storage map (JSON + PNG per
  channel, font pools and cursor in `config.json`/`font-pool-cursor.json`, accounts in
  `social/connections.json` + the 0600 secrets file). No SQLite table exists.
- `src-tauri/src/import/{plan,apply}.rs` — read-only scan, per-image SHA-256
  verification, plain skip reasons, byte-verified copies, `imports/ledger.json`
  idempotency, and a `source_unchanged` proof that fails naming the file if Story's
  bytes change.
- Commands `story_import_preview` / `story_import_run`, an in-app folder browser
  (`list_dirs`), and the agent-equivalent route `POST /api/test/trigger/import`.
- Commits `df13436` and `0659f1c`. Full lib suite **39/39**.

## Not done (named, not hidden)

## Real import report — `little-weather-within` (2026-11-21)

Run against the staged copy at
`_orchestra/youtube-stack/wire-still-posts-fixture-20261121/little-weather-within/`
(40 files, 47,223,288 bytes; the hand-over `README-fixture.md` is excluded).

| | |
|---|---|
| Channels with Still Posts | **1** (`little-weather-within`); **30 without** — a true report |
| Posts scanned | 9 (`post-0001`…`post-0009`) |
| Imported | **9** |
| Skipped | **0** |
| Images copied | 30, byte-verified (posts 0007–0009 carry the 1:1 pair only; imported without complaint) |
| Bytes copied | 47,164,542 |
| Source unchanged | **true** (every source file's mtime, size and SHA-256 identical before and after) |
| Second run | 0 copied, 9 `already_present` — the ledger works |
| Ledger | `imports/ledger.json`, keyed by channel + post id + content hash |

The run is reproducible with
`cargo test --lib real_channel_report -- --ignored --nocapture`.

### Defects found by the real data (not by the fixture)

1. **A `font-pool-cursor.json` that does not exist anywhere was required.** Found when the
   first real scan failed on a file Production does not have. Fix: removed the lookup;
   fonts come from `config.json` when present and a channel without font pools still
   imports.
2. **`project.json` was required, and this channel never carried one.** Found on the same
   scan. Fix: `project.json` and `config.json` are optional; the channel's identity falls
   back to its folder name.
3. **A queue test asserted lateness exactly.** Found only on the second full-suite run
   (29 vs 30 minutes from float `julianday` rounding). Fix: accept one minute of rounding.

These three are the reason the import is honest rather than merely green: a fixture could
not have found any of them.

## What this proof is, and what it is not

This is **proven through the core test on a temp data dir**: the scan and apply ran as
unit tests against the staged Production channel and a scratch Wire data directory.
The binary build and a run driven through the app's own import route still owe one pass,
deferred while Hub's gate holds the heavy lane; publishing 0.2.0 waits for that pass.
- Recipes link by id rather than writing `wire.db` post rows.
- Imported recipes carry `renderable:false` because Wire has no layer renderer yet.
- Account/token import needs a secrets schema change and belongs with the next batch.
- No binary rebuild or route run: the heavy lane is Hub's, deferred by the Developer.
- UI import screen: in progress (second slice).

*Wire Studio - Lead - 001, 2026-11-21.*
