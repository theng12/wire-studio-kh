# Changelog

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
