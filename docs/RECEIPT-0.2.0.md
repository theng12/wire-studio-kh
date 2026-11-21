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

- No real import yet: Production's inventory (read on story-mini, 2026-11-21) is
  **1 channel populated with Still Posts and 30 without** — `little-weather-within`,
  9 posts; the library root is
  `/build/Documents/Story Studio KH/Story Studio Projects` across 31 live
  channels. That is a true report, not a failure. Staging of that one channel to this
  Mac is in progress; the importer never guesses a library root.
- Recipes link by id rather than writing `wire.db` post rows.
- Imported recipes carry `renderable:false` because Wire has no layer renderer yet.
- Account/token import needs a secrets schema change and belongs with the next batch.
- No binary rebuild or route run: the heavy lane is Hub's, deferred by the Developer.
- UI import screen: in progress (second slice).

*Wire Studio - Lead - 001, 2026-11-21.*
