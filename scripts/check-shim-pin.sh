#!/bin/bash
# The vendored Sparkle shim library must be the one the shim repo says is current.
# The framework binary has its own hash and never equals this pin — check libkhsparkle.a only.
set -u
cd "$(dirname "$0")/.."
SHIM="${SPARKLE_SHIM:-$HOME/Developer/_orchestra/developer-system/sparkle-shim-20261123}"
pin="$(awk '/^build1:/ {print $2}' "$SHIM/evidence/ARTIFACTS.txt" 2>/dev/null)"
mine="$(shasum -a 256 src-tauri/vendor/khsparkle/libkhsparkle.a 2>/dev/null | cut -d' ' -f1)"
if [ -n "$pin" ] && [ "$pin" = "$mine" ]; then
  echo "PASS shim pin $pin"
else
  echo "FAIL shim pin: vendored ${mine:-missing}, shim says ${pin:-unknown}"
  exit 1
fi
