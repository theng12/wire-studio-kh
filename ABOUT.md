# Wire Studio KH — what it is

Wire Studio KH posts your social updates: one caption, sent to whichever accounts you
pick, each named the way you named it rather than by a technical key. It queues every
send, shows what happened to each one, and reads the models it uses from Gateway.

## What it does

- Composes one caption and sends it now or at a chosen time (Phnom Penh time in the
  window, UTC in the queue).
- Understands each place it posts to as its own target, with the platform's own limits:
  Telegram channels, YouTube uploads and Facebook Pages. A place that cannot take a
  text post says so instead of offering a dead button.
- Creates one queue row per place-and-size pair, so a double post is impossible and you
  can see each attempt separately: queued, sending, sent, failed with the reason, or
  needs attention.
- Retries transient failures with backoff, honours a platform's own Retry-After, and
  stops at a permanent refusal with the platform's words. Retry and Cancel say why when
  they cannot be used.
- Connects to Gateway from Settings: paste the address and key, press Connect, and read
  back what Gateway answered — how many models it publishes, an empty catalogue, or its
  own refusal. A key that is revoked there turns the card to "connection failed" and asks
  you to check again.
- Imports Still Posts from a Story Studio KH channel: point it at the channel folder,
  preview what would come in, then copy it. Story is only ever read, never changed.
- Answers an update hold on `127.0.0.1:8649` so the Launcher can slide an update into a
  gap; in-flight posts are named and resumed, never cancelled.
- Keeps credentials in its own 0600 file. No Keychain, no provider keys, no secrets in
  the window or in the feed.
- Tells the truth on screen: "not configured", "saved, not verified" and "connected" are
  three different states, and a metric or count with no data never reads as zero.

## How it is built

Tauri shell with a Rust core and a TypeScript web UI:

- `src-tauri/src/` — queue store (SQLite), dispatch loop, delivery adapters
  (`adapters/`), format presets, the Gateway client (`gateway.rs`), the update hold
  (`hold.rs`), the 0600 secrets file, the Story import (`import/`), and a headless
  test-mode server (`testmode.rs`).
- `ui/` — one window: Composer, Queue, Targets, Import and Settings.
- `scripts/` — the release build, publish-and-read-back, and the proofs the gate runs
  (test-mode smoke, update hold, Gateway connect, import).

Build the app with `scripts/build-release.sh`; publish with `scripts/publish.sh`, which
reads the served feed back from the publisher host.

## Where it fits

Wire owns your social accounts' credentials and the queue; Story Studio KH owns the
channel library it imports from; Gateway owns models, images and stock, so Wire holds
none of those provider keys. Instagram is designed and shelved; TikTok returns after the
platform batches. Images, video posts and the Media Studio catalogue ride 0.5.0 and
later.
