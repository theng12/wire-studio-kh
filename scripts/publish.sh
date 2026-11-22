#!/bin/bash
# Publish the current VERSION to the signed feed on 1301 and read it back.
# The feed entry for `wire` already exists after 0.1.0, so no --new-app.
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

# The publisher is base/current, never bin/khu: bin/khu on 1301 was the Nov 21 binary with no
# --type, and a stale copy that silently lacks a flag ships a row without the field. The guard
# below names the binary and the missing flag instead of letting that happen.
KHU='~/Library/KHUpdater/base/current'
KHU_HELP="$(ssh -i "$KEY" -o BatchMode=yes "$HOST" "$KHU publish --help 2>&1" || true)"
for flag in --type --tagline --hold-since; do
  case "$KHU_HELP" in
    *"$flag"*) ;;
    *) echo "FAIL publisher $KHU on $HOST has no $flag — refusing to publish" >&2; exit 1 ;;
  esac
done
echo "publisher: $KHU on $HOST (--type, --tagline, --hold-since all present)"

ssh -i "$KEY" -o BatchMode=yes "$HOST" "mkdir -p ~/$REMOTE_DIR"
scp -q -i "$KEY" "$DMG" "$ROOT/CHANGELOG.md" "$HOST:~/$REMOTE_DIR/"

ssh -i "$KEY" -o BatchMode=yes "$HOST" "
set -e
K=~/Library/KHUpdater
$KHU publish \
  --feed \$K/feed --key \$K/keys/private.key \
  --app wire --version $VERSION \
  --artifact ~/$REMOTE_DIR/'Wire Studio KH_${VERSION}_aarch64.dmg' \
  --changelog ~/$REMOTE_DIR/CHANGELOG.md \
  --name 'Wire Studio KH' \
  --type media \
  --tagline 'Posts one caption to your social accounts.' \
  --description 'Composes one caption and sends it to your social accounts, each named by you rather than a technical key, with a Gateway connection you can check at any time.' \
  --kind app-dmg
"

echo "--- feed readback from 1301 ---"
PLUGIN="$(ls -d "$HOME"/.claude/plugins/cache/studio-orchestration/studio-orchestration/*/ | tail -1)"
"${PLUGIN}scripts/feed-verify.sh" wire --expect-version "$VERSION" --expect-sha "$SHA"
