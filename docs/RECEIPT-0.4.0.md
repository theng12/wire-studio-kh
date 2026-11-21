# Wire Studio KH 0.4.0 — Telegram connect flow (2026-11-21)

**State: published 2026-11-21T15:20:58Z and read back from 1301 with a matching SHA.**
Connecting a Telegram channel no longer needs a browser or a person to explain it.

## What is in it

- **Connect.** Paste the bot token, press Connect: the app calls `getMe` and either
  saves the token and says "Connected as @your_bot", or shows Telegram's refusal word
  for word and saves nothing.
- **Find my channel.** A `getUpdates` call lists every channel the bot can see — title
  and kind, no JSON. An empty list says exactly what to do: add the bot as an
  administrator, post a message, press Check again. Nothing polls by itself.
- **Paste what you have.** `@username`, a `t.me` link or the numeric id all resolve
  through `getChat`, and the title is shown back before saving. Invite links and post
  links get a plain explanation instead of a failure.
- **The title is saved with the id** and used everywhere: the card reads
  `connected · @bot → Wire Smoke Test`, the composer's target reads
  `Telegram — Wire Smoke Test`, and Settings says "Sending to …".
- **How to connect**, on every platform card: one collapsed line showing the step you
  are on ("1 of 4" before a token, "3 of 4: add the bot as an administrator" after),
  the bot's own handle filled in once the token verifies, and the block disappears
  when connected. YouTube and Facebook carry the same pattern.

## Proof

| Check | Result |
|---|---|
| `cargo test` | 64 passed, 0 failed, 1 ignored |
| Connect flow route proof (`scripts/telegram-connect-proof.sh`) | 9/9 PASS — bad token refused in Telegram's words, Find needs the operator credential, Use refuses unconnected, invite link explained, a name resolved through Telegram and refused, nothing saved by a refusal |
| Browser flow (headless UI, stubbed core) | Connect → "Connected as @wire_studio_kh_bot"; Find → 2 channels by title and kind; Use → "Sending to Wire Smoke Test", list closes |
| Composer after connecting | target reads "Telegram — Wire Smoke Test connected" |
| How-to block | step 1 before a token; hidden when connected |
| Public artifact scan | 0 hits, PASS |
| Secret scan | PASS |
| Signature | `KH Studio Local Signing`, verifies |
| Feed read-back from 1301 | PASS — `wire` 0.4.0, sha `be8a544c…`, bundleID/holdURL/icon intact |

## Not done / not checked

- **No live Telegram connection yet.** The success path needs a real bot token; this
  run proved the refusals, the parsing and the UI with a stubbed answer. The owner's
  own walkthrough with his bot is the remaining proof, and the screen is built so it
  needs nobody's help.
- YouTube and Facebook connections still ship in a later build (their cards say so).
- Install on a second Mac not checked; Hub's mirror allowlist still pending.

## Published artifact

| | |
|---|---|
| DMG | `Wire Studio KH_0.4.0_aarch64.dmg`, 4,551,259 bytes |
| SHA-256 | `be8a544ce666e495bfca8b20241418bb8258fd3722357921e2c51a997034ea13` |
| Retention | `dist/` keeps 0.4.0 (current) + 0.3.1 (previous); 0.3.0 retired to Desktop `_trash` with a manifest |

*Wire Studio - Lead - 001, 2026-11-21.*
