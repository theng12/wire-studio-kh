# Wire Studio KH 0.2.1 — public-artifact fix (2026-11-21)

**State: published 2026-11-21T14:19:51Z.** A fix on top of 0.2.0, found by the public-mirror
scan. 0.3.0 work continues separately and is not in this build.

## What was embedded, and what is not now

0.2.0's binary carried **576 absolute `/Users/<name>` paths** (cargo registry source
paths baked into dependency panic strings) at offsets around 7,368,000 in
`Contents/MacOS/Wire Studio KH`. Anyone who downloaded the app could read them.

Fix at the build, not the artifact:
- Release build now sets `RUSTFLAGS="--remap-path-prefix=$HOME=/build
  --remap-path-prefix=$TMPDIR=/tmp"`, so every embedded compile-time path is rewritten.
- Release profile strips symbols (`strip = true`, no debug info).
- Rebuilt from a clean worktree at the 0.2.0 commit (`fix/0.2.1-path-leak`, `b9675da`) so
  in-progress 0.3.0 edits could not enter the release.

Result: the 0.2.1 binary has **0 hits** under the precise rule below.

## The scan rule, made precise and tested both ways

`scripts/scan-public.sh`:

- **Fails:** absolute user paths (`/Users/…`), tailnet hostnames/URLs (`*.ts.net`,
  `terranash-*`), personal email addresses, and key/token material.
- **Allows:** `terranash` **only** as part of `com.terranash.` — the public bundle id, which
  is on the feed and in every install by design. Exact-prefix exemption with that comment,
  never a blanket ignore of the word.
- Two reviewed third-party licences/CA contacts are allowed by exact address
  (`appro@openssl.org`, `info@e-szigno.hu`), never by domain; icon filenames such as
  `128x128@2x.png` are not read as emails.

Self-test (`scan-public.sh --self-test`), all six PASS: synthetic `/Users/x/y` caught;
synthetic hostname caught; synthetic email caught; `com.terranash.wirestudiokh` allowed;
icon filename not misread; reviewed third-party contact allowed. Against the shipped
0.2.0 DMG the same scan reports **576 hits, every one `/Users/<name>`** — the check
catches a real leak and passes a clean build.

## Published artifact

| | |
|---|---|
| DMG | `Wire Studio KH_0.2.1_aarch64.dmg`, 4,519,021 bytes |
| SHA-256 | `dba6ce3d4b50aa58aee3fc2a6ee8e67fe1c85855f80e29fe6f4b3076fc55e526` |
| Signature | `KH Studio Local Signing`, signed app + DMG, not notarized |
| Public scan | PASS — 0 hits |
| Preflight | PASS |
| Feed read-back | PASS from 1301; `wire` 0.2.1, bundleID + holdURL intact, previous 0.2.0 |
| Retention | `dist/` holds 0.2.1 (current) + 0.2.0 (previous); 0.1.0 retired to Desktop `_trash` with a manifest |

## Not done / not checked

- The fix commit is on branch `fix/0.2.1-path-leak` and **not yet merged into `main`**:
  `main` holds in-progress 0.3.0 work from four writers and is not in a mergeable state.
  Merge follows 0.3.0, or earlier if the Developer wants the order changed.
- The mirror itself is Hub's tool, not mine; `wire` still needs adding to the page-sync
  allowlist before the public page can carry it.
- The shipped-window click-through and the live Telegram post remain the owner's two items.
- Not checked: install through the Launcher on a second Mac.

*Wire Studio - Lead - 001, 2026-11-21.*
