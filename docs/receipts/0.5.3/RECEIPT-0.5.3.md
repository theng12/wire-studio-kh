# Release receipt — wire 0.5.3

app: wire
version: 0.5.3
commit: 4f4c840e996ece847fd6a21c9e0d23dc04e1e8c4
artifact: $HOME/Developer/_orchestra/developer-media/wire-studio-kh/app/dist/Wire Studio KH_0.5.3_aarch64.dmg
sha256: 90b8f5784538dbbbeef684a0fd1ff9af868d83ba1406d22e558b5da31b3bb319
bytes: 6120843
scan: sha256=90b8f5784538dbbbeef684a0fd1ff9af868d83ba1406d22e558b5da31b3bb319 clean — scan-dmg/2026-11-20, exit 0, 89 files, excused: none
gate config: $HOME/Developer/_orchestra/tools/gate-configs/wire-0.5.3.sh (MINUTES=20)
gate exit: 0 — "GATE PASS wire 4f4c840e996ece847fd6a21c9e0d23dc04e1e8c4 2026-11-24T09:13:55Z"
gate runner, pasted from the run's own summary.txt:
    runner: gate-run.sh f05b010c16e5 at $HOME/Developer/_orchestra/tools/gate-run.sh
    the file at that path hashed f05b010c16e577986c796341a43b41b450e4639b82159f664c15b7b89d06bd5a at the end, unchanged; a rename would not have corrupted this run, an in-place rewrite would have (Release Tooling measured both)
feed readback: wire 0.5.3 — fresh anonymous read of https://theng12.github.io/kh-releases/release.json: version 0.5.3, sha256 90b8f5784538dbbbeef684a0fd1ff9af868d83ba1406d22e558b5da31b3bb319, 6,120,843 bytes, tagline "Posts one caption to your social accounts.", holdURL http://127.0.0.1:8649/api/maintenance/hold, holdSince 0.1.0, url .../releases/download/v0.5.3/Wire.Studio.KH_0.5.3_aarch64.dmg
page state: same — the served row's url, file, length and sha are the published asset and the gated bytes
dirty: false
retired: 0.4.3 and 0.5.1 — 10,695,623 bytes moved to ~/Desktop/_trash/wire-studio-kh/2026-11-24 superseded installers/ with MANIFEST.md (nothing freed until the owner empties it)
not proven: written below

## Closeout

retired: `Wire Studio KH_0.4.3_aarch64.dmg` (4,579,444 B, sha 34b42272…) and
`Wire Studio KH_0.5.1_aarch64.dmg` (6,116,179 B, sha acbc4544…) moved to
`~/Desktop/_trash/wire-studio-kh/2026-11-24 superseded installers/` with a MANIFEST.md — 10,695,623 bytes
moved, freed from `dist/` only when the owner empties the trash. Kept on purpose: 0.5.3 and 0.5.2 (current +
previous), 0.4.2 (the Developer's rule: retire only once 0.5.2's install is confirmed — it is not), and
0.5.0 (the version installed on this Mac, which predates Sparkle and so cannot re-derive itself).
Evidence counts: 25 gate directories under `lead/` (29 MB), 209 log files, none older than two days —
nothing else retired.

## Tag provenance — corrected on the owner's word (2026-11-24)

`refs/tags/v0.5.3` was moved from `866a5abe6a7e8fb07de69917e16da4e6f92f9037` to
`4f4c840e996ece847fd6a21c9e0d23dc04e1e8c4`, authorised by the owner through Developer - Media - M001 for
this one ref and not as a pattern. Why: the tag had been cut from remote HEAD before the 0.5.3 work
existed, so it named 0.5.2's commit. Readback after the move, from outside the checkout:
`git ls-remote --tags origin v0.5.3` → `4f4c840…`; `gh api …/contents/app-meta.json?ref=v0.5.3` →
`"localFile": "Wire Studio KH_0.5.3_aarch64.dmg"`; release asset unchanged at 6,120,843 B and
sha256 `90b8f578…`; `git diff --stat v0.5.3 4f4c840` empty. `raw.githubusercontent.com` served the old
filename for minutes afterwards (CDN lag), so the API is the proof, not raw.

Before the move:

`refs/tags/v0.5.3` named `866a5abe6a7e8fb07de69917e16da4e6f92f9037`; the gated commit is
`4f4c840e996ece847fd6a21c9e0d23dc04e1e8c4`. `git diff --stat v0.5.3 4f4c840` lists 21 files (311
insertions, 28 deletions) — the whole 0.5.3 change set — so the tag names 0.5.2's tree while the asset is
0.5.3's build. Cause: the release was created before main was pushed. Owner's decision, prepared both ways
in `lead/0.5.3/TAG-PROVENANCE.md`. The artifact's provenance is `4f4c840`, proven by the gate that built
it and by the sha256 the feed row carries.

## Evidence

- gate log: $HOME/Developer/_orchestra/developer-media/wire-studio-kh/lead/gate-053-run/gate-run3.log
- gate evidence dir: $HOME/Developer/_orchestra/developer-media/wire-studio-kh/lead/gate-20261124-161049-38690
- readback command and output: fresh anonymous read of the feed, then of the appcast, compared field by
  field — row url == appcast enclosure url (True), row bytes == appcast length (True, 6120843), edSignature
  88 chars, tagline 42 chars, hold pair with the :8649 route and holdSince 0.1.0. Watched until it changed:
  the row was 0.5.2 at 09:20 and served 0.5.3 by attempt 5 of the poll (about 100 s later).
- install acceptance: UNMEASURED — this Mac still runs 0.5.0 (/Applications/Wire Studio KH.app, 2026-11-23 14:17), which predates Sparkle, so installing is the Launcher's job; nothing was hand-installed.

## Voided gate attempts tonight (each with its own INVALID.txt, none is a failure)

- lead/gate-20261124-151701-30117 — started at head 4450dddd5, superseded.
- lead/gate-20261124-154117-88729 — aborted before any step to correct one sentence in CHANGELOG.md (head 8fd60126b → 4f4c840e9).
- lead/gate-20261124-154310-4326 — voided before any step: the shared runner was edited in place while this gate waited on the lane (5f368fb48cab at start, 7cbdbe31 after).
- lead/gate-20261124-155010-26585 — aborted before any step to re-declare the lane wait at 20 minutes instead of 60.
- The real attempt is lead/gate-20261124-161049-38690.

## §5d screenshots

Three images, captured from the release build in this session, hidden (`open -g -j`, window read by id,
`screencapture -l`), never brought to the front, no pointer and no keystroke; fixture in a throwaway data
folder at `/tmp/wire-studio-kh-demo`, deleted after each run.

- queue-dark.png — 103,632 bytes — read by eye: queue rows for two demo posts, no names, emails, tokens,
  hosts, tailnet or real paths.
- composer-dark.png — 84,012 bytes — read by eye: empty composer with the Telegram target chip; same result.
- settings-dark.png — 94,805 bytes — read by eye: data folder `/tmp/wire-studio-kh-demo/data`, Updates
  "ready"; same result. The first capture leaked `/var/folders/<machine hash>/…` here and was replaced.
- README embeds two and links the third with alt text; `scripts/check-screenshots.sh` PASS.
- Light appearance not captured: three was the floor and the app opens in the system's dark appearance.
- The capture is now a repo script, `scripts/capture-screenshot.sh`, so the next release repeats it.
- Better route recorded for next release (Relay's, from M001): headless Chrome over CDP against a
  disposable preview fixture, no window at all — `lead/BACKLOG.md`.

## What this version is

- The README shows the app: queue, composer, settings.
- Nothing about posting changed.
- Release notes are the two-sentence summary in `lead/0.5.3/RELEASE-NOTES.md`, not the changelog.

## Not proven

- Updated by itself — no installed copy has changed its version by itself; the Mac runs 0.5.0.
- Windowed idle footprint — the gate recorded `UNMEASURED (set IDLE_PID in the gate config)`.
- Packaged SIGTERM acceptance, and U001's three-configuration table row 2 — deferred since 0.4.2.
- Light-appearance capture — three dark images only.
