#!/bin/bash
# Publish Wire Studio KH 0.1.0 to the signed feed on 1301 and read it back.
# Run after `npx tauri build --bundles dmg` produced the DMG.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(cat "$ROOT/VERSION")"
HOST="${KH_PUBLISH_HOST:?set KH_PUBLISH_HOST to the publisher's address}"
KEY="$HOME/.ssh/id_ed25519_gateway1301"
DMG="$ROOT/src-tauri/target/release/bundle/dmg/Wire Studio KH_${VERSION}_aarch64.dmg"
REMOTE_DIR="wire-studio-${VERSION}"

[ -f "$DMG" ] || { echo "FAIL no DMG at $DMG" >&2; exit 1; }
SHA="$(shasum -a 256 "$DMG" | awk '{print $1}')"
BYTES="$(stat -f%z "$DMG")"
echo "artifact: $DMG"
echo "sha256:   $SHA"
echo "bytes:    $BYTES"

ssh -i "$KEY" -o BatchMode=yes "$HOST" "mkdir -p ~/$REMOTE_DIR"
scp -q -i "$KEY" "$DMG" "$ROOT/CHANGELOG.md" "$HOST:~/$REMOTE_DIR/"

ssh -i "$KEY" -o BatchMode=yes "$HOST" "
set -e
K=~/Library/KHUpdater
~/Library/KHUpdater/bin/khu publish \
  --feed \$K/feed --key \$K/keys/private.key \
  --app wire --version $VERSION \
  --artifact ~/$REMOTE_DIR/'Wire Studio KH_${VERSION}_aarch64.dmg' \
  --changelog ~/$REMOTE_DIR/CHANGELOG.md \
  --name 'Wire Studio KH' \
  --kind app-dmg \
  --hold-url 'http://127.0.0.1:8649/api/maintenance/hold' \
  --allow-same
"

echo "--- feed readback from 1301 ---"
PLUGIN="$(ls -d "$HOME"/.claude/plugins/cache/studio-orchestration/studio-orchestration/*/ | tail -1)"
"${PLUGIN}scripts/feed-verify.sh" wire --expect-version "$VERSION" --expect-sha "$SHA"
