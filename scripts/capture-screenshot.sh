#!/bin/bash
# §5d screenshot capture, inside the release: one screen from the built .app, hidden.
#   bash scripts/capture-screenshot.sh <section> <outfile> [hidden|bg]
# Hidden first (open -g -j) and the window read by id; "bg" only if that gives nothing, and that
# fallback belongs in the receipt. No pointer, no keystroke, nothing brought to the front.
#
# The fixture is throwaway and lives at a fixed neutral path, never under TMPDIR: Settings prints the
# data folder and the token path, and /var/folders/<machine hash>/ in a public image names the Mac.
set -uo pipefail
cd "$(dirname "$0")/.."

SECTION="${1:?section, e.g. queue}"
NAME="${2:?outfile, e.g. queue-dark.png}"
MODE="${3:-hidden}"
APP="$PWD/src-tauri/target/release/bundle/macos/Wire Studio KH.app"
BIN="$APP/Contents/MacOS/Wire Studio KH"
[ -x "$BIN" ] || { echo "FAIL no release build at $APP — run scripts/build-release.sh first"; exit 1; }
OUT=docs/screenshots
mkdir -p "$OUT"

FIX=/tmp/wire-studio-kh-demo
DATA="$FIX/data"
TMPHOME="$FIX/home"
rm -rf "$FIX"
mkdir -p "$DATA" "$TMPHOME/Library/Application Support/com.terranash.wirestudiokh"
# A demo hold token, so the Updates card shows the normal ready state, not a red missing chip.
printf 'demo-token\n' >"$TMPHOME/Library/Application Support/com.terranash.wirestudiokh/hold-token"
chmod 600 "$TMPHOME/Library/Application Support/com.terranash.wirestudiokh/hold-token"
PORT=$(python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')
TOKEN="capture-$$"

# Demo content: two queued posts with a test channel, written by the app's own route. The operator
# credential goes to this process too, or every trigger answers 403 and the shots come out empty.
WIRE_STUDIO_KH_DATA_DIR="$DATA" WIRE_OPERATOR_TOKEN="$TOKEN" \
  "$BIN" --test-mode --KH_TEST_PORT "$PORT" >"$DATA/testmode.log" 2>&1 &
TM=$!
for _ in $(seq 1 60); do curl -fsS -m 1 "http://127.0.0.1:$PORT/api/test/state" >/dev/null 2>&1 && break; sleep 0.1; done
seeded=0
for text in "Sunrise over the krama loom" "Afternoon tea, freshly picked"; do
  curl -sS -m 5 -X POST "http://127.0.0.1:$PORT/api/test/trigger/schedule" \
    -H "x-wire-operator: $TOKEN" -H 'Content-Type: application/json' \
    --data "{\"text\":\"$text\",\"targets\":[\"telegram:TESTCHAT\"],\"delay_s\":900}" \
    | grep -q '"ok":true' && seeded=$((seeded + 1))
done
kill "$TM" 2>/dev/null; wait "$TM" 2>/dev/null
[ "$seeded" = 2 ] || { echo "FAIL seeded $seeded of 2 posts (see $DATA/testmode.log)"; exit 1; }

# The window's own id, so the capture is that window and nothing else. Compiled per run; /usr/bin/env
# is not involved, so this is a throwaway binary in the fixture dir.
cat >"$FIX/winid.swift" <<'SWIFT'
import CoreGraphics
import Foundation
let pid = Int(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
    if (w[kCGWindowOwnerPID as String] as? Int) == pid, (w[kCGWindowLayer as String] as? Int) == 0 {
        if let id = w[kCGWindowNumber as String] as? Int { print(id); exit(0) }
    }
}
exit(1)
SWIFT
swiftc -O "$FIX/winid.swift" -o "$FIX/winid" 2>/dev/null || { echo "FAIL could not build the window-id helper"; exit 1; }

if [ "$MODE" = hidden ]; then FLAGS=(-g -j); else FLAGS=(-g); fi
open "${FLAGS[@]}" --env WIRE_STUDIO_KH_DATA_DIR="$DATA" --env WIRE_START_SECTION="$SECTION" \
  --env HOME="$TMPHOME" --env WIRE_OPERATOR_TOKEN="$TOKEN" "$APP" || { echo "FAIL open refused"; exit 1; }

PID=""
for _ in $(seq 1 80); do
  # Anchored: an unanchored pattern matches the owner's /Applications copy and captures that window.
  PID="$(pgrep -f "^$BIN" | head -1)"
  [ -n "$PID" ] && break
  sleep 0.25
done
[ -n "$PID" ] || { echo "FAIL the app did not start"; exit 1; }

ID=""
for _ in $(seq 1 80); do
  ID="$("$FIX/winid" "$PID" 2>/dev/null)" && [ -n "$ID" ] && break
  sleep 0.25
done
if [ -z "$ID" ]; then
  kill -TERM "$PID" 2>/dev/null
  echo "NO-WINDOW $MODE"
  exit 3
fi
sleep 2   # let the first state render land
screencapture -x -o -l "$ID" "$OUT/$NAME" || { kill -TERM "$PID" 2>/dev/null; echo "FAIL screencapture"; exit 1; }
kill -TERM "$PID" 2>/dev/null
for _ in $(seq 1 40); do kill -0 "$PID" 2>/dev/null || break; sleep 0.1; done
kill -9 "$PID" 2>/dev/null
rm -rf "$FIX"
[ -s "$OUT/$NAME" ] && echo "CAPTURED $OUT/$NAME $(stat -f %z "$OUT/$NAME") bytes (window $ID, mode $MODE)"
