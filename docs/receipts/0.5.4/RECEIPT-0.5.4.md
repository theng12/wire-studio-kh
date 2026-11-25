# Release receipt — wire 0.5.4

app: wire
version: 0.5.4
commit: eba75407e4923e44f4c0e777d4d47f082da00714 (tag v0.5.4; gated build commit a3b29b5ec3df698451a3a35adb207c9b99222b4c, post-gate metadata-only commit eba75407e4923e44f4c0e777d4d47f082da00714 proven byte-neutral)
artifact: ~/Developer/_orchestra/developer-media/wire-studio-kh/app/dist/Wire Studio KH_0.5.4_aarch64.dmg
sha256: 62b663a667fd97286d7be1d7e743d01d97ea36d0266adbdb30834d83fe704b5c
bytes: 6118694
scan: sha256=62b663a667fd97286d7be1d7e743d01d97ea36d0266adbdb30834d83fe704b5c clean — scan-dmg/2026-11-20, exit 0, 89 files, excused: none
gate config: ~/Developer/_orchestra/tools/gate-configs/wire-0.5.4.sh (MINUTES=20)
gate exit: 0 — "GATE PASS wire a3b29b5ec3df698451a3a35adb207c9b99222b4c 2026-11-25T05:14:13Z"
gate runner, pasted from the run's own summary.txt:
    runner: gate-run.sh 528de9fe53d0 at ~/Developer/_orchestra/tools/gate-run.sh
    sha256 before and after run: 528de9fe53d0c0de552d30dca1b21092b073a24fac76daf53e21da738b3b6f46 (unchanged)
feed readback: UNMEASURED — kh-releases feed still serves 0.5.3 pending Main/S001 feed pass with the five row values; Wire's own Pages appcast https://theng12.github.io/wire-studio-kh/appcast.xml verified live serving 0.5.4 (sha256 d7d114f5647c44372aaa28a6442c22b2d6a365d2eac42334f949e75fec4b5add matching docs/appcast.xml, anonymous enclosure download 6,118,694 B sha256 62b663a667fd97286d7be1d7e743d01d97ea36d0266adbdb30834d83fe704b5c, EdDSA signature verified)
page state: GitHub release v0.5.4 published non-draft at commit eba75407e4923e44f4c0e777d4d47f082da00714 with asset 62b663a667fd-Wire.Studio.KH_0.5.4_aarch64.dmg (6,118,694 B, sha256 62b663a667fd97286d7be1d7e743d01d97ea36d0266adbdb30834d83fe704b5c, release-asset-name-check.sh PASS, app-meta.json 10 fields verified at ref=v0.5.4); Pages appcast live at commit 2b676b7a772206ec82f1e9a8d152202e10b47eb9
dirty: false
retired: 0.4.2, 0.5.0, 0.5.2 installers from dist/ (15,279,294 B) + 2 unreferenced 0.5.4 scratch DMGs from lead/0.5.4/ (12,237,200 B) — 27,516,494 bytes total moved to ~/Desktop/_trash/wire-studio-kh/2026-11-25 superseded installers/ with MANIFEST.md (disk space returns only when the owner empties trash)
not proven: written below

## Closeout

- **Published release:** `theng12/wire-studio-kh` tag `v0.5.4` (non-draft, non-prerelease) at `eba75407e4923e44f4c0e777d4d47f082da00714`, published via `packaging/github-asset.sh`.
  - Asset: `62b663a667fd-Wire.Studio.KH_0.5.4_aarch64.dmg` (`6,118,694` bytes, sha256 `62b663a667fd97286d7be1d7e743d01d97ea36d0266adbdb30834d83fe704b5c`).
  - `release-asset-name-check.sh theng12/wire-studio-kh v0.5.4`: PASS (`62b663a667fd-Wire.Studio.KH_0.5.4_aarch64.dmg`).
  - `gh api repos/theng12/wire-studio-kh/contents/app-meta.json?ref=v0.5.4`: verified 10 fields (`name`, `bundle_id`, `bundle_name`, `type`, `tagline`, `description`, `hold_url`, `hold_since`, `kind`, `localFile`).
- **Appcast on Pages:** `docs/appcast.xml` committed at `2b676b7a772206ec82f1e9a8d152202e10b47eb9`, pushed to `work/wire-054-sidebar` and `main`, deployed by GitHub Pages at `2026-11-25T05:49:40Z`. Served XML sha256 `d7d114f5647c44372aaa28a6442c22b2d6a365d2eac42334f949e75fec4b5add` matches `docs/appcast.xml`; `sign_update --verify` passes over the served `sparkle:edSignature` and a fresh anonymous download of the enclosure URL (`6,118,694` B, sha256 `62b663a667fd97286d7be1d7e743d01d97ea36d0266adbdb30834d83fe704b5c`).
- **Post-gate metadata-only commit (`eba7540`):** bumped `package.json` and `package-lock.json` to `0.5.4` and added `bundle_name`, `hold_url`, `hold_since`, `kind` to `app-meta.json`. Verified none of those three files is embedded in `Wire Studio KH.app` (`Contents/Info.plist` carries `0.5.4`, `Contents/Resources/` contains only `icon.icns`, `scripts/build-release.sh` only greps `app-meta.json` for `localFile`); the gated DMG bytes at `a3b29b5` are unaffected.
- **Retention & retirement:** `dist/` now holds `0.5.4` + `0.5.3` (current + previous). Retired `Wire Studio KH_0.4.2_aarch64.dmg` (4,551,905 B), `Wire Studio KH_0.5.0_aarch64.dmg` (4,608,819 B), `Wire Studio KH_0.5.2_aarch64.dmg` (6,118,570 B), `gate1-failed-build.dmg` (6,118,629 B), and `pre-gate-build-for-screenshot.dmg` (6,118,571 B) — `27,516,494` bytes total moved recoverably to `~/Desktop/_trash/wire-studio-kh/2026-11-25 superseded installers/` with `MANIFEST.md`. Safe because `/Applications/Wire Studio KH.app` is now confirmed on `0.5.3`.
- **Evidence counts under `lead/`:** 26 gate directories, 292 log files (31 older than two days, left untouched to preserve prior receipts' citations), ~32 MB remaining after moving the two scratch DMGs.

## Evidence

- Gate evidence dir: `~/Developer/_orchestra/developer-media/wire-studio-kh/lead/gate-20261125-121317-88519` (`summary.txt`, `GATE-PASS.txt`, `step-sidebar-footer.log`).
- Layout measurements (`step-sidebar-footer.log`, `SUMMARY_SIDEBAR fails=0`):
  - `1440x900` (Composer and Settings): `.sidebar-footer` bottom `892 <= 900`, `docScrollH 900 == docClientH 900`, `pageScrollH 1433` (Settings scrolls inside `.page`), `'Updates'` headings: `1` (`['This app', 'Connections', 'Update hold', 'Gateway', 'Updates', 'Appearance', 'About']`).
  - `1280x800` (Composer and Settings): `.sidebar-footer` bottom `792 <= 800`, `docScrollH 800 == docClientH 800`, `pageScrollH 1433`, `'Updates'` headings: `1`.
  - Headless screenshots: `~/Developer/_orchestra/developer-media/wire-studio-kh/lead/0.5.4/layout/settings-sidebar-1440x900.png` (93,606 B) and `settings-sidebar-1280x800.png` (79,184 B).
- `scripts/prepublish-check.sh`: `PASS prepublish check for 0.5.4` (signature verified, hold route `:8649`, tagline 42 chars, 3 README screenshots `<= 400 KB`).
- `release-preflight.sh` (plugin `0.3.340`, `36ebc8155c2e`): `PREFLIGHT PASS` at `eba7540` (with waiver for pre-existing 0.5.3 receipt path in `docs/receipts/0.5.3/RECEIPT-0.5.3.md`; none introduced by 0.5.4).
- Install acceptance: `UNMEASURED` — `/Applications/Wire Studio KH.app` currently runs `0.5.3`; installing `0.5.4` is the Launcher's job after Main/S001 publishes the `kh-releases` feed row.

## What this version is

- The sidebar footer ("Support this app", the version pill, and What's New) now stays pinned in view at every window size on every page while only the page content scrolls.
- In Settings, the update-hold status card is now titled "Update hold" so it no longer shares the "Updates" heading with the update check card.

## Not proven

- `kh-releases` signed feed row update and `feed-verify.sh` readback — awaiting Main/S001's feed pass (five row values handed over in `lead/0.5.4/ROW-VALUES.md`).
- Automatic self-update from `0.5.3` to `0.5.4` via Studio Launcher KH / Sparkle — follows the feed pass.
- Windowed idle footprint — gate recorded `UNMEASURED (set IDLE_PID in the gate config)`.
- Packaged SIGTERM acceptance, and U001's three-configuration table row 2 — carried from prior releases.
