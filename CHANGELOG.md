# Changelog

## 0.5.4 — 2026-11-25

- **The sidebar stays in the window.** Support this app, the version pill and What's New now sit at the bottom of the sidebar at every window size and on every page; only the page content scrolls, never the whole window.
- **Two Settings cards both said "Updates".** They are different things — whether Wire may pause its queue for an update, and the update check itself — so the first one is now called **Update hold**.

## 0.5.3 — 2026-11-24

- The README now shows Wire: the queue you land on, the composer where a post starts, and the two Settings cards that carry a real choice — Gateway and Updates.
- Nothing about posting changed in this release; its job is to carry those pictures and the release-notes rule for the Launcher card.

## 0.5.2 — 2026-11-23

- **Wire can update itself now.** The Sparkle helper in 0.5.1 could not read a `~/…` token path, so every update was refused before the check ran. 0.5.2 carries the fixed helper, and Settings says which state the update token is in.
- **The update token lives with Wire's own data.** It moves to `~/Library/Application Support/com.terranash.wirestudiokh/hold-token`. The old shared location is copied there once, with a line saying so, and is never read again.
- **No token means the hold refuses and says why** — in the app's own words, naming the file it looked for — instead of waiting for an update to fail.

## 0.5.1 — 2026-11-23

- **Wire updates itself.** The app now carries Sparkle, the studio's one update mechanism: it checks the signed appcast, downloads a newer version and swaps it in while the app is relaunched. There is no window, no prompt and nothing to click — the same behaviour as the other KH apps.
- **The queue is held before anything is checked.** Wire first asks its own hold route to pause dispatch; only a hold that answers yes lets the check run. If the hold cannot be taken, the update is refused and the app keeps working.
- **The hold now refuses an unauthenticated caller** with `401` instead of answering as if it were safe. A request with no token, or the wrong one, pauses nothing.
- The app's signature covers Sparkle's framework and its nested helpers, verified with `--deep --strict` at build time and again inside the finished DMG.

## 0.5.0 — 2026-11-22

- **Products from Media Studio, read-only.** Wire now reads Media Studio's catalogue straight from its own store — nothing is copied and nothing is written to it. The Import screen lists your active products with their price and the time that price was read ("price as of 12:34"), searchable by name or SKU. Archived products are not listed.
- **Attach a product to a post.** The Composer has a product picker. The chosen product's name and price appear under the field and in the preview line, and the post can still be queued without one.
- **The price is read again at send.** If a price changed between composing and sending, the sent text carries the new price with the time it was read — never a frozen one. The queue keeps both numbers: the price you were shown and the price that was sent, so a change is visible rather than silent.
- **Honest prices.** No currency symbol is invented (Media records none), a product with no price says "price unavailable", and a store that cannot be read says why instead of showing an empty list or a zero.
- Wire reads Media's store read-only: the main database file's bytes and modification time are checked before and after, and the proof also shows Wire writes nothing to the write-ahead log.

## 0.4.3 — 2026-11-22

- Settings has a **Gateway** card: paste Gateway's address and API key and press **Connect**. Wire asks Gateway for its live model list and says exactly what came back — "connected · 51 models available", "connected · no models published yet", or Gateway's own words, for example "Invalid API key.".
- A key that stops working (rotated or revoked at Gateway) is never shown as connected: the card reads **connection failed** with Gateway's sentence, and the button becomes **Check connection**. A saved key is shown only as `•••• abcd`.
- An unreachable Gateway says "Nothing is listening at http://…" instead of a request error, and a check that hangs gives up after 10 seconds.
- Nothing polls: Wire asks Gateway only when you press the button, and the saved answer is a fact with a time, never a cached "connected".
- The word "core" is gone from the last screens that still said it (empty lists, verification reasons, the offline banner).
- The Import screen's **Choose…** browser is now proven headless as well as in the window: it lists real folders, and an unreadable folder says why. The Settings Browse… stays gone — Wire's data folder is fixed when the app starts, so a button there could not work.

## 0.4.2 — 2026-11-21

- Destinations carry their names, never a key: "Wire Smoke Test", "YouTube channel", "Facebook page" — in the Queue and the Composer, with the technical id kept only as a tooltip.
- The word "core" is gone from every sentence you can see, including the footer and the offline message.
- A Retry or Cancel that cannot be used now says why on the row itself — "sent, nothing to retry", "cancel is closed after the first attempt" — instead of only greying out.
- The dead Browse… button is removed until choosing a folder actually works.
- The Composer now says what it does: one caption, text only for now. Sizes that need a video or an image are not offered, and their destinations say so plainly.

## 0.4.1 — 2026-11-21

- Every destination is named the way you see it on Targets: the Queue and the Composer now say "YouTube", "Facebook Page" and "Telegram — Wire Smoke Test" instead of internal ids.
- The Queue is shorter and easier to scan: a post takes the room it needs, the channel and state sit in fixed columns, and the next attempt is two short lines under the state.
- Plain words replace internal ones: "Where Wire keeps its posts and queue", "Not connected yet", "Not checked yet" — no mentions of a core, a bridge or a signed feed.
- A button that cannot be used now looks disabled and says why; Browse… is disabled until folder choosing ships.
- Small fixes: "One caption" instead of "Plain text only", a shorter channel example, and the missing space before a comma in the Telegram steps.

## 0.4.0 — 2026-11-21

- Connecting a Telegram channel is now done in the app, by anyone, with no browser and no help: paste the bot token from @BotFather and press **Connect** — the app answers "Connected as @your_bot", or Telegram's own refusal word for word.
- Press **Find my channel** to see every channel the bot can see, listed by name with its kind, and pick one. If the list is empty the card says exactly what to do — add the bot as an administrator, post a message, then **Check again**.
- Anything else you have also works: paste `@username`, a `t.me` link or the number; the app checks it with Telegram and shows the channel title back for confirmation before saving.
- The channel name is saved with the id and used everywhere afterwards — the card reads "connected · @your_bot → Wire Smoke Test" and the composer reads "Sending to Wire Smoke Test", never a bare number.
- Every platform card carries a collapsed **How to connect** block that shows only the step you are on, and disappears once you are connected. YouTube and Facebook use the same pattern.

## 0.3.1 — 2026-11-21

- The Telegram card stops contradicting itself: it now says **connected** only after Telegram itself confirms the bot and the channel, **saved, not verified** when both are stored but unconfirmed, and shows Telegram's own refusal word for word when the check fails.
- Saving a token and channel now asks Telegram straight away: who the bot is, and whether it can see that channel. A network miss never loses what you saved — it says "saved, not verified" and why.
- The version shown in the footer, About and Settings now comes from the app build itself, not a hand-written string, so the footer always names the version you are actually running.
