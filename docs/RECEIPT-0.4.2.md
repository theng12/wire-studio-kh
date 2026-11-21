# Wire Studio KH 0.4.2 — the visual-pass rules, completed (2026-11-21)

**State: published 2026-11-21T15:53:58Z and read back from 1301 with a matching SHA.**
0.4.1 fixed the nine faults as listed; three of them were then raised to rules and
finished here. Nothing from the list is dropped.

## Fault by fault — before → after

| id | before | after | shipped |
|----|--------|-------|---------|
| V1 | Queue and Composer named destinations `youtube:chan`, `facebook_page:page`, `telegram:-1004483436341` | destinations read **"Wire Smoke Test"**, **"YouTube channel"**, **"Facebook page"**; the raw id is only a tooltip | 0.4.1, tightened 0.4.2 |
| V2 | an enabled "Browse…" beside "Browse… is not wired yet" | the button **and its handler are removed** until folder choosing actually works | 0.4.2 |
| V3 | "core", "bridge", "signed feed" on screen (footer, offline banner, schedule help, errors, What's New) | plain sentences; the word core appears nowhere in user copy | 0.4.1, finished 0.4.2 |
| V4 | Post 112 px, rows 142 px, "Next attempt (UTC+7)" wrapping 3 deep, Actions 189 px with buttons clipped | Post 200 px, rows **84–122 px measured**, State carries the attempt and next attempt in two short lines, Actions 184 px, both buttons inside the table | 0.4.1 |
| V5 | "Plain text only" beside presets asking for a video or an image | the composer is **one caption, text only**: media presets are not offered, and their destinations say "needs a video or an image — ships in a later build" | 0.4.2 |
| V6 | a disabled Retry looked identical to a live one | dashed and muted **plus the reason on the row**: "sent, nothing to retry" / "queued, nothing to retry" / "cancel is closed after the first attempt" | 0.4.1 style, 0.4.2 reason |
| V7 | channel placeholder clipped | `@my_channel or a t.me link` | 0.4.1 |
| V8 | step 1 rendered `/newbot ,` | the stray comma is gone | 0.4.1 |
| V9 | "Connection shipping in a later build" repeated on two cards | the page subtitle says it once | 0.4.1 |

## Proof

- Nine states re-rendered: `_orchestra/youtube-stack/visual-pass-20261121/AFTER/`
  (before: the same nine in `pages/`). Re-measured in the browser at 1200×800:
  row heights `84, 113, 113, 122` px; columns `200 / 180 / 170 / 184`; every action
  button's right edge inside the table.
- `cargo test`: **64 passed**, 0 failed, 1 ignored (the real-channel import test).
- Public artifact scan: 0 hits. Secret scan: PASS.
- Feed read-back from 1301: `wire` 0.4.2 with a matching sha (below).

## Not done / not checked

- The Telegram connect flow's live success path still needs a real bot token; see
  the 0.4.0 receipt. Nothing here changes that.
- The folder browser, the What's New dialog beyond a first view, and the card
  (non-table) Queue layout were not part of the nine shots; the card layout reads
  the same data through the same label and reason code.
- YouTube and Facebook connections still ship later; their destinations are
  intentionally not selectable for a text post, which is what the screen now says.

## Published artifact

| | |
|---|---|
| DMG | `Wire Studio KH_0.4.2_aarch64.dmg`, 4,551,905 bytes |
| SHA-256 | `228cd10c26fb4ed8edd5eeefba4333e7e2af45f8290d74819d3d7b7c0b4dde86` |
| Feed file | `228cd10c26fb-Wire Studio KH_0.4.2_aarch64.dmg` |
| Retention | `dist/` keeps 0.4.2 (current) + 0.4.1 (previous); 0.4.0 retired to Desktop `_trash` with a manifest |

*Wire Studio - Lead - 001, 2026-11-21.*
