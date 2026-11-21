# Changelog

## 0.3.1 — 2026-11-21

- The Telegram card stops contradicting itself: it now says **connected** only after Telegram itself confirms the bot and the channel, **saved, not verified** when both are stored but unconfirmed, and shows Telegram's own refusal word for word when the check fails.
- Saving a token and channel now asks Telegram straight away: who the bot is, and whether it can see that channel. A network miss never loses what you saved — it says "saved, not verified" and why.
- The version shown in the footer, About and Settings now comes from the app build itself, not a hand-written string, so the footer always names the version you are actually running.

## 0.3.0 — 2026-11-21

- Two more places to post: a YouTube channel (the video uploads straight to the channel and picks up where it stopped if the connection drops) and a Facebook Page (a text post, or one image that lives at a public web address).
- Format presets: each place now has named sizes to choose from — YouTube landscape and Shorts, Facebook square, portrait and link card, Telegram text — and the composer shows the size, ratio, text limit and safe area before anything is sent.
- One post, many places and many sizes: every place-plus-size pair is its own line in the queue, so you can see each one separately and a double post is still impossible.
- Instagram is designed and shelved: nothing is built and it never appears in the app.
- Refusals stay honest: a Facebook size on a YouTube channel is refused by name, Facebook says plainly when an image has no public web address, and sign-in or permission problems go to Needs attention instead of retrying forever.
- The adapter tests now cover success, throttling, cut text, permanent refusal and sign-in expiry for every platform, so retry behaviour is proven before a real account is connected.

## 0.2.1 — 2026-11-21

- Build hardening: the app no longer carries this Mac's home folder or build paths inside it. Release builds now rewrite compile-time paths to `/build` and strip symbols, so a downloaded copy reveals nothing about the machine that built it.

## 0.2.0 — 2026-11-21

- New Import screen: point it at a Story Studio KH channel, see exactly what would come in, then copy the posts and images into Wire. Story's files are only ever read, never changed.
- Every image is checked against the checksum Story recorded before it is copied; anything that fails is skipped and says why.
- Importing the same channel twice changes nothing the second time.
- On the real Production channel this brought in all 9 posts and all their artwork with nothing skipped. That channel is the only one of 31 that had Still Posts.
- Imported posts are kept as they were drawn and marked "not yet re-renderable in Wire" until Wire can draw its own layers.

## 0.1.0 — 2026-11-21

- First release: the app opens, and you can write a text post, pick a Telegram channel, schedule it, and see what happened on each target.
- The queue is the home screen. Every post is one row per channel, so a double post is impossible.
- The app can be held for an update: it stops taking new work, lets in-flight posts settle, and resumes by itself.
- Secrets live in one locked-down file on this Mac. No Keychain, no password prompts.
