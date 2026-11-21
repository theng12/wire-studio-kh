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

Two defects the real data exposed and this receipt records: the importer required a
`font-pool-cursor.json` that does not exist, and it required `project.json` that the
Production channel does not carry — both now optional, and the channel's identity falls
back to its folder name.
- Recipes link by id rather than writing `wire.db` post rows.
- Imported recipes carry `renderable:false` because Wire has no layer renderer yet.
- Account/token import needs a secrets schema change and belongs with the next batch.
- No binary rebuild or route run: the heavy lane is Hub's, deferred by the Developer.
- UI import screen: in progress (second slice).

*Wire Studio - Lead - 001, 2026-11-21.*
