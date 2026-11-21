# Wire Studio KH 0.3.0 — Platforms (2026-11-21)

**State: published 2026-11-21T14:52:40Z and read back from 1301 with a matching SHA.**
Builds on 0.2.1. The owner's pick for the app mark is in this build.

## What is in it

- **YouTube upload**: two-step resumable upload with a token cache and refresh, so a
  dropped connection resumes instead of starting over.
- **Facebook Pages**: Graph v21 feed and photos posts. Status mapping is code-aware
  (401/OAuthException → sign-in, 403 and codes 200/10 → permission, 429 and codes
  4/17/32/613 → throttled with `Retry-After`, 5xx → platform outage, transport →
  network). The page token is scrubbed from every error message. An image with only
  a local path refuses in plain words: "Facebook needs a public web address for this
  post".
- **Format presets** (`src-tauri/src/formats.rs`): one registry — `yt-16x9`,
  `yt-shorts`, `fb-1x1`, `fb-4x5`, `fb-16x9`, `tg-text` — each with size, ratio,
  text limit and safe area. `fb-16x9` declares `40:21` because 1200×630 is Meta's
  link-card size, not a true 16:9 frame; the label says "Facebook link card".
- **Variant fan-out**: schema **v2** adds `preset` to delivery rows with
  `UNIQUE(post_id, target, preset)`, so one post can hold several sizes per place
  and a double post is still impossible. Migration is forward-only: v1 rows copy
  over with a NULL preset; an older app refuses a v2 database.
- **Composer**: a preset picker per target with the size, ratio, limit and safe
  area shown before sending.
- **Scripted adapter matrix** (`adapters/stub.rs`): success, throttling with
  `Retry-After`, truncation, permanent refusal and auth expiry for every platform.
- Instagram stays **shelved**: designed, not built, never listed.

## The icon — the owner's pick

The app's old icon read as a prohibited sign; the owner rejected it. Three options
were drawn at 256 and 32 px and sent. **The owner picked option A, the dispatch
line**, and said it is good for the logo.

- Drawn **per size**, never scaled down: full line with start node at 128 px and
  above, a heavier line at 48–127 px, a simplified one-bend arrow at 32 and 16 px.
- `icon.icns` slots each get their own render (16→32, 32→64, 128→256, 256→512,
  512→1024); alpha is transparent outside the rounded square.
- The same mark is used inside the app: sidebar header, About, and empty states
  (`ui/logo.svg`, from the same generator).
- Options B and C moved to `docs/icon-options/rejected/`; only A is the app's.
- The published feed icon was fetched back and checked: it is the new mark
  (`iconSha256 a8fdb33e…`).

## Proof run for this build

| Check | Result |
|---|---|
| `cargo test` | **59 passed**, 0 failed, 1 ignored (the real-channel import test, by design) |
| Fan-out route proof (`scripts/fanout-route-proof.sh`) | 8/8 PASS — one row per target+preset, a repeated target deduped, a Facebook preset on YouTube refused by name, operator credential required |
| Headless smoke (`scripts/test-mode-smoke.sh`) | 12/12 PASS, run three times |
| Secret scan of the DMG (`scan-dmg.sh`) | PASS, no secret-like strings |
| Public artifact scan (`scan-public.sh`) | **0 hits**, PASS |
| Public scan self-test | 6/6 PASS |
| Signature | `KH Studio Local Signing`, identifier `com.terranash.wirestudiokh`, verifies `--deep --strict` |
| Feed read-back from 1301 | PASS — `wire` 0.3.0, bundleID, holdURL, icon and iconSha256, notes, sha match |

Two defects were found and fixed while proving this build:

1. The headless schedule route stored no preset when the caller named none; it now
   defaults to the platform's first preset, exactly what the composer pre-selects.
2. The headless smoke raced the app's three startup lines; it now waits for the
   last one.

## Merge order

`fix/0.2.1-path-leak` (`b9675da`) was merged into `main` before this build
(merge commit `030ec13`); `git merge-base --is-ancestor b9675da main` passes. The
0.3.0 commits (`409d142` code, `cb8ee08` icon) sit on top of that merge.

## Published artifact

| | |
|---|---|
| DMG | `Wire Studio KH_0.3.0_aarch64.dmg`, 4,479,599 bytes |
| SHA-256 | `904d8cb49c22bf4783708a1cd2dd4d49ba96118f726f45ce55e3f6e644e04b4c` |
| Feed file | `904d8cb49c22-Wire Studio KH_0.3.0_aarch64.dmg` |
| Retention | `dist/` holds 0.3.0 (current) + 0.2.1 (previous); 0.2.0 retired to `~/Desktop/_trash/wire-studio-kh/2026-11-21 0.2.0 dmg/` with a MANIFEST.md |

## Not done / not checked

- **No live post to YouTube or Facebook**: there is no channel or page token on this
  Mac. Adapter behaviour is proven by the scripted matrix and the route proofs, not
  by a real upload.
- The shipped-window click-through (composer → preset picker → queue) and a live
  Telegram post remain the owner's two items; the headless route and the browser
  preview of the header/About mark were checked instead.
- The public mirror/allowlist is Hub's tool; `wire` still needs adding to the
  page-sync allowlist before the public page carries it.
- Not checked: install through the Launcher on a second Mac.
- Imported Story recipes remain `renderable:false` until Wire draws its own layers.

*Wire Studio - Lead - 001, 2026-11-21.*
