# Release receipt — wire 0.5.2

app: wire
version: 0.5.2
commit: 4acd51647d6b8c892173034f08f1a6dffa185c21
artifact: dist/Wire Studio KH_0.5.2_aarch64.dmg
sha256: d6bb096b9b99daa13f8c876361162788008aa55624ba7090c3d0c9cb052de809
bytes: 6118570
gate config: $HOME/Developer/_orchestra/tools/gate-configs/wire-0.5.2.sh
gate exit: 0 — "GATE PASS wire 4acd51647d6b8c892173034f08f1a6dffa185c21 2026-11-23T15:34:27Z"; runner sha256 b6b44ee87a31315b6acda6b558ce5f02f2d92e0de61ba33d43a93a8167aab320 unchanged before and after
feed readback: at the time of writing the signed feed still serves wire 0.5.1 (sha acbc4544…); 0.5.2's row arrives with the next feed pass
page state: behind — 0.5.1 served, 0.5.2 published and appcast-served
dirty: false
scan: PASS — scripts/scan-public.sh, hits=0 on the artifact and on this receipt
retired: none — 0 bytes
not proven: a Mac updating by itself (needs an installed copy); U001's table row 2 (the shipped GUI binary started directly — it opens a window on this Mac); windowed idle footprint (UNMEASURED); packaged SIGTERM acceptance (deferred since 0.4.2)

## What changed, and why it can update

- `src-tauri/vendor/khsparkle/libkhsparkle.a` is the shim's current build, pinned by
  `scripts/check-shim-pin.sh` against the shim's own `ARTIFACTS.txt`; the framework binary has its own
  hash and is deliberately not compared.
- The hold token lives at `~/Library/Application Support/com.terranash.wirestudiokh/hold-token`; the
  estate path is copied there once and never read again; with neither file the hold answers 401 and
  Settings says where it looked.
- 0.5.1 could not update itself (`libkhsparkle.a` = `62ca951049daf27ea60dec7cf468a079ef00adf1f00f17b2fef8853318db870e`
  and `KHHoldTokenFile = ~/Library/KHUpdater/hold-token`); 0.5.2 is the first version that can.

## Sparkle states, kept apart

- served: yes — the appcast answers anonymously and carries the 0.5.2 item whose enclosure names the
  published asset `d6bb096b9b99-Wire.Studio.KH_0.5.2_aarch64.dmg`; that URL answers 200 at 6,118,570 bytes
  with the sha above.
- item signed: yes — verified over the exact DMG bytes with `sign_update --verify --ed-key-file`.
- updated by itself: **not run** — no installed copy has changed its `CFBundleShortVersionString` by itself.

## Evidence

- gate log: `$HOME/Developer/_orchestra/developer-media/wire-studio-kh/lead/gate-20261123-221916-32188/summary.txt` (untracked)
- an earlier PASS from `gate-20261123-221338-72134` is void: `gate-run.sh` changed mid-run
  (0e313409… → b6b44ee8…, issue row 20261123-155); its artifact (`d95718c9…`) is not this release.
- readback: anonymous fetch of the appcast (0.5.2 item present) and of the release asset (200, sha matched)
- install acceptance: UNMEASURED — the Launcher installs it; no installed copy has been read back.
