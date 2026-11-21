# Wire Studio KH 0.4.1 — visual pass fixes (2026-11-21)

**State: published 2026-11-21T15:37:47Z and read back from 1301 with a matching SHA.**
The first studio visual pass over Wire: nine faults found, nine fixed, every page
re-shot and the layout re-measured.

The pass folder is `_orchestra/youtube-stack/visual-pass-20261121/` — `FINDINGS.md`,
`PLAN.md`, `pages/` (before, 9 shots) and `AFTER/` (after, 4 shots).

## What changed

- **Names, not keys.** The Queue and the Composer label every destination as Targets
  does — "YouTube", "Facebook Page", "Telegram — Wire Smoke Test" — with the raw id
  kept only as a tooltip. The state chip uses one word per state.
- **Queue measured and rebuilt.** Post column 112 px → 216 px; the State column now
  carries the attempt and the next attempt in two short lines; rows **142 px → 86 px**;
  Actions fits its buttons with room instead of clipping them.
- **Plain words.** "Where Wire keeps its posts and queue", "Not connected yet", "Not
  checked yet" — no "core", "bridge" or "signed feed" on screen.
- **Honest controls.** Browse… is disabled with its reason; a disabled Retry/Cancel
  now looks disabled (dashed, muted) instead of identical to a live one.
- Small: "One caption" replaces "Plain text only"; the channel example fits; the
  comma after `/newbot` is fixed; the duplicated "shipping in a later build" line is
  gone (the page subtitle says it once).

## Proof

| Check | Result |
|---|---|
| Re-render + re-measure | queue rows 86 px, Post 216 px, Actions 184 px with both buttons visible in light and dark |
| Composer target names | "Telegram — Wire Smoke Test connected", "YouTube not configured", "Facebook Page not configured" |
| Public artifact scan | 0 hits, PASS |
| Secret scan | PASS |
| Feed read-back from 1301 | PASS — `wire` 0.4.1, sha `c1107cf1…` |
| Rust suite | unchanged since 0.4.0's 64-pass run; this release changed `ui/` only |

## Not done / not checked

- The connect flow's live success path still needs the owner's bot token (see the
  0.4.0 receipt); nothing here changes that.
- The repeated fault "a key shown where a name belongs" is filed once as row
  `20261121-258` in the issues directory, so the next app inherits the check.
- Shot list covers 9 page/state combinations, not every dialog; the What's New and
  Support dialogs were seen, the folder browser was not.

## Published artifact

| | |
|---|---|
| DMG | `Wire Studio KH_0.4.1_aarch64.dmg`, 4,551,303 bytes |
| SHA-256 | `c1107cf153a39b73c42945794b7d6feeb7142c247e02b8b59e542a0d3d31cac3` |
| Retention | `dist/` keeps 0.4.1 (current) + 0.4.0 (previous); 0.3.1 retired to Desktop `_trash` with a manifest |

*Wire Studio - Lead - 001, 2026-11-21.*
