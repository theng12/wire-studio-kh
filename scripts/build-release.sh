#!/bin/bash
# The only release build path. Every compiled-in path is rewritten to /build and
# symbols are stripped, so the DMG carries no local paths — the rule the mirror
# scan enforces (scripts/scan-public.sh). Paths come from the caller's own $HOME,
# never a committed absolute path.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
export RUSTFLAGS="--remap-path-prefix=$HOME=/build --remap-path-prefix=${TMPDIR:-/tmp}=/tmp"
exec npx tauri build --bundles dmg "$@"
