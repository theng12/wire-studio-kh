# Wire Studio KH 0.3.1 — one truth on the Telegram card (2026-11-21)

**State: published 2026-11-21T15:07:34Z and read back from 1301 with a matching SHA.**
Two UI defects found on the owner's own screen, both fixed.

## What changed

1. **The Telegram card told two stories.** With a token and channel saved it showed
   a "not configured" chip beside a sentence saying both were saved. Now:
   - Saving asks Telegram itself: `getMe` (who is the bot) and `getChat` (can it see
     that channel). The answer and its time are stored in `settings.json`.
   - One label per state, used by the card, the composer chip and Settings:
     **connected** (Telegram confirmed), **saved, not verified** (stored, never
     confirmed), **connection failed** (with Telegram's own words), **not configured**.
   - A bare `telegram:<chat>` composer target takes the Telegram block's verdict, so
     the composer cannot say "not configured" about a channel the card calls saved.
2. **The footer showed a version that was not installed.** The footer pill, About,
   Settings hint and window title now take the version from the core's build
   (`CARGO_PKG_VERSION`); the JavaScript constant is only a fallback when no core
   answers. The stale 0.1.0 string is gone from the UI.

## Proof

| Check | Result |
|---|---|
| `cargo test` | 61 passed, 0 failed, 1 ignored |
| Save → verify, real route, fake token | saved, then `connection_state: failed` with `Telegram HTTP 401 Unauthorized: Unauthorized (check the bot token)` |
| Card states in the browser (stubbed core) | `saved, not verified`, `connected`, `connection failed` — chip, reason and composer target agree in all three |
| Footer / About / title | read `0.3.1` from the core state |
| Headless smoke | 12/12 PASS |
| Fan-out route proof | 8/8 PASS |
| Public artifact scan | 0 hits, PASS |
| Secret scan | PASS |
| Signature | `KH Studio Local Signing`, verifies |
| Feed read-back from 1301 | PASS — `wire` 0.3.1, sha match, bundleID/holdURL/icon intact |

A real Telegram channel was not available to this run: the failure path was proven
with a deliberately invalid token, and the success path is proven by unit tests over
the parsed answers (`connected · @bot → channel title`) plus the browser states. A
live save against the owner's own bot is the next real proof and is part of 0.4.0's
connect flow.

## Published artifact

| | |
|---|---|
| DMG | `Wire Studio KH_0.3.1_aarch64.dmg`, 4,507,783 bytes |
| SHA-256 | `fc3aeb81f04caca0c2099f6e6dd478454d99b6520a9be1d7e9481d5bb566d1e8` |
| Feed file | `fc3aeb81f04c-Wire Studio KH_0.3.1_aarch64.dmg` |
| Icon | unchanged from 0.3.0 (owner's pick, `a8fdb33e06e3-icon.png`) |
| Retention | `dist/` holds 0.3.1 (current) + 0.3.0 (previous); 0.2.1 retired to `~/Desktop/_trash/wire-studio-kh/2026-11-21 0.2.1 dmg/` with a MANIFEST.md |

## Not done / not checked

- The 0.3.0 receipt's open items stand (no live YouTube/Facebook post; install on a
  second Mac not checked; Hub's mirror allowlist).
- **Next, 0.4.0:** the in-app Telegram connect flow — token → Connect, "Find my
  channel" over `getUpdates`, accept `@username` / `t.me/…` / numeric id, persist the
  channel title and show "Sending to <title>", with a collapsed "How to connect"
  block whose current step is the one shown; the same pattern for YouTube and
  Facebook. Then the studio `visual-pass` over every page, fault list reported before
  any fix.

*Wire Studio - Lead - 001, 2026-11-21.*
