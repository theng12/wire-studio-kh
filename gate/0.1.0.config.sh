#!/bin/bash
# Wire Studio KH 0.1.0 gate config for gate-run.sh. A release adds a config, never a new gate script.

APP="wire-studio-kh"
WORKTREE="/build/Developer/My Own Apps/wire-studio-kh"
export PATH="$HOME/.cargo/bin:$PATH"

STEPS=(
  "changelog-heading::grep -q \"^## $(cat VERSION) \" CHANGELOG.md"
  "queue-unit-tests::cd src-tauri && cargo test --lib queue:: -- --test-threads=1"
  "feedcheck-unit-tests::cd src-tauri && cargo test --lib feedcheck::"
  "no-native-dialogs::! grep -REn 'window\\.(confirm|alert|prompt)\\(' ui/"
  "no-keychain::! grep -REn 'SecItem|Keychain|security-framework' src-tauri/src"
  "test-mode-smoke::bash scripts/test-mode-smoke.sh"
  "hold-proof::bash scripts/hold-proof.sh"
  "hold-expiry::bash scripts/hold-proof.sh --expiry-only"
)

# Set IDLE_PID (the running core) to measure the 10-minute idle assert.
RESTORE=()
