#!/bin/bash
# Wire Studio KH 0.2.0 gate config for gate-run.sh. A release adds a config, never a new gate script.

APP="wire-studio-kh"
# The checkout is the repo this config ships in, or WIRE_WORKTREE: a config carries no machine path.
WORKTREE="${WIRE_WORKTREE:-$(git -C "${CFG:+$(dirname "$CFG")}" rev-parse --show-toplevel 2>/dev/null)}"
export PATH="$HOME/.cargo/bin:$PATH"

STEPS=(
  "changelog-heading::grep -q \"^## $(cat VERSION) \" CHANGELOG.md"
  "queue-unit-tests::cd src-tauri && cargo test --lib queue:: -- --test-threads=1"
  "feedcheck-unit-tests::cd src-tauri && cargo test --lib feedcheck::"
  "import-unit-tests::cd src-tauri && cargo test --lib import::"
  "no-native-dialogs::! grep -REn 'window\\.(confirm|alert|prompt)\\(' ui/"
  "no-keychain::! grep -REn 'SecItem|security-framework|SecKeychain' src-tauri/src"
  "test-mode-smoke::bash scripts/test-mode-smoke.sh"
  "hold-proof::bash scripts/hold-proof.sh"
  "hold-expiry::bash scripts/hold-proof.sh --expiry-only"
  "import-route::bash scripts/import-route-proof.sh"
)

# Set IDLE_PID (the running core) to measure the 10-minute idle assert.
RESTORE=()
