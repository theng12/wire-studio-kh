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

- No real import: this Mac has **no live Still Posts channel** and no Story library
  root. The importer never guesses one; Production's inventory is being read on
  story-mini and the report will be built from what comes back.
- Recipes link by id rather than writing `wire.db` post rows.
- Imported recipes carry `renderable:false` because Wire has no layer renderer yet.
- Account/token import needs a secrets schema change and belongs with the next batch.
- No binary rebuild or route run: the heavy lane is Hub's, deferred by the Developer.
- UI import screen: in progress (second slice).

*Wire Studio - Lead - 001, 2026-11-21.*
