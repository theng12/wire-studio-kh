# Release receipt — wire 0.5.1

app: wire
version: 0.5.1
commit: 57ef4ef64f31bb732592d96bca57377400a39d1a
artifact: dist/Wire Studio KH_0.5.1_aarch64.dmg
sha256: acbc4544bd2e8903597ed213f9764d094d08bc5771ae4b5b503ed9ddab6bdc6e
bytes: 6116179
gate config: $HOME/Developer/_orchestra/tools/gate-configs/wire-0.5.1.sh
gate exit: 0 — "GATE PASS wire 57ef4ef64f31bb732592d96bca57377400a39d1a 2026-11-23T13:22:52Z"; runner sha256 bc0dbcd8c3621012aea845de04714e4295496a426b94ea928ab5c2ea0c68f8f3 unchanged before and after
feed readback: wire 0.5.1 — the served version since 2026-11-23T14:28:29.342Z; sha256 acbc4544bd2e8903597ed213f9764d094d08bc5771ae4b5b503ed9ddab6bdc6e, 6,116,179 bytes, type media, tagline "Posts one caption to your social accounts.", holdURL http://127.0.0.1:8649/api/maintenance/hold, holdSince 0.1.0
page state: same
dirty: false
scan: PASS — scripts/scan-public.sh, hits=0 on the artifact and on this receipt
retired: none — 0 bytes
not proven: a Mac updating by itself (needs an installed copy; a served appcast and a valid signature do not imply it); windowed idle footprint (UNMEASURED); packaged SIGTERM acceptance (deferred since 0.4.2)

## Sparkle states, kept apart

- served: yes — the appcast answers 200 anonymously with the real document and an enclosure naming the
  published asset; the feed row is the served version since 2026-11-23T14:28:29.342Z.
- item signed: yes — the `sparkle:edSignature` verifies over the exact DMG bytes against the
  `SUPublicEDKey` read out of the shipped bundle (CryptoKit check) and, independently, against
  `~/.kh-sparkle/com.terranash.wirestudiokh/private.pub` (openssl, Ed25519 over the raw 32-byte key).
- updated by itself: **not run** — no installed copy has changed its `CFBundleShortVersionString` by
  itself.

## What this version is, and is not

**0.5.1 is released and installable; self-update begins at 0.5.2.** It cannot update itself, and that
was established statically rather than found in the field:

- `src-tauri/vendor/khsparkle/libkhsparkle.a` ships the pre-fix build,
  sha256 `62ca951049daf27ea60dec7cf468a079ef00adf1f00f17b2fef8853318db870e`, which cannot expand a `~`
  in a token path.
- `src-tauri/Info.plist` carried `KHHoldTokenFile = ~/Library/KHUpdater/hold-token`.

So the hold was refused before Sparkle reached the feed. 0.5.2 carries the fixed library and moves the
token to Wire's own data directory.

## Closeout

retired: none — 0 bytes. `dist/` keeps 0.5.0 (previous) beside 0.5.1 (current). 0.4.3 and 0.4.2 also
remain there on purpose: they predate the public repo, so they are the only copies of those artifacts,
and are not retired without the owner's word.

## Evidence

- gate log: `$HOME/Developer/_orchestra/developer-media/wire-studio-kh/lead/gate-20261123-201905-16229/summary.txt` (untracked, kept where the gate wrote it)
- readback command and output: an anonymous read of the signed feed (`release.json`, base64 `feed`) returned
  wire version 0.5.1 with the sha and byte count above; the appcast was fetched anonymously (HTTP 200) and
  its signature verified over `dist/Wire Studio KH_0.5.1_aarch64.dmg`.
- install acceptance: UNMEASURED — the Launcher installs it; no installed copy has been read back.
