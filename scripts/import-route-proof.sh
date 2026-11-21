#!/bin/bash
# Drives the Still Posts import through the app's own route (not a unit test),
# against the staged Production channel, and proves Story's files never change.
# Usage: import-route-proof.sh [channel-dir]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHANNEL="${1:-/build/Developer/_orchestra/youtube-stack/wire-still-posts-fixture-20261121/little-weather-within}"
BIN="${WIRE_BIN:-$ROOT/src-tauri/target/debug/wire-studio-kh}"
[ -x "$BIN" ] || { echo "FAIL binary missing: $BIN"; exit 1; }
[ -d "$CHANNEL/still-posts/posts" ] || { echo "FAIL not a Still Posts channel: $CHANNEL"; exit 1; }

fails=0
pass() { echo "PASS $1"; }
bad() { echo "FAIL $1${2:+ — $2}"; fails=$((fails+1)); }

tmp="$(mktemp -d /tmp/wire-import-route.XXXXXX)"
port="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
pid=""
cleanup() { [ -n "$pid" ] && kill -TERM "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; rm -rf "$tmp"; }
trap cleanup EXIT

# Story's bytes before the run.
snapshot() { find "$CHANNEL" -type f -exec shasum -a 256 {} + | sort; }
snapshot > "$tmp/before.txt"

if lsof -nP -iTCP:8649 -sTCP:LISTEN >/dev/null 2>&1; then
  echo "FAIL hold port 8649 in use by another process" >&2; exit 1
fi

WIRE_STUDIO_KH_DATA_DIR="$tmp/data" "$BIN" --test-mode --KH_TEST_PORT "$port" > "$tmp/log" 2>&1 &
pid=$!
for _ in $(seq 1 50); do grep -q '^TEST_URL=' "$tmp/log" && break; sleep 0.2; done
grep -q '^TEST_URL=' "$tmp/log" && pass "test-mode server ready on $port" || { bad "test-mode server ready"; exit 1; }
url="http://127.0.0.1:$port"
token="$(python3 -I -c "import json;print(json.load(open('$tmp/data/app-secrets.json'))['operator_token'])" 2>/dev/null)"
[ -n "$token" ] && pass "operator token available" || { bad "operator token available"; exit 1; }

code="$(curl -sS -m 10 -o "$tmp/noauth" -w '%{http_code}' -X POST "$url/api/test/trigger/import" \
  -H 'Content-Type: application/json' --data "{\"root\":\"$CHANNEL\"}")"
[ "$code" = "403" ] && pass "import route refuses a caller without the operator credential" \
  || bad "import route refuses a caller without the operator credential" "HTTP $code"

code="$(curl -sS -m 120 -o "$tmp/preview" -w '%{http_code}' -X POST "$url/api/test/trigger/import" \
  -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
  --data "{\"root\":\"$CHANNEL\",\"apply\":false}")"
posts="$(python3 -I -c "import json;print(json.load(open('$tmp/preview')).get('posts',''))" 2>/dev/null)"
[ "$code" = "200" ] && [ "$posts" = "9" ] && pass "preview reports 9 posts" \
  || bad "preview reports 9 posts" "HTTP $code posts=$posts"

code="$(curl -sS -m 300 -o "$tmp/run" -w '%{http_code}' -X POST "$url/api/test/trigger/import" \
  -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
  --data "{\"root\":\"$CHANNEL\",\"apply\":true}")"
python3 -I - "$tmp/run" <<'PY' > "$tmp/assert.txt" || true
import json,sys
d=json.load(open(sys.argv[1]))
r=d.get("report",{})
c=r.get("counts",{})
print("imported", c.get("imported"))
print("skipped", c.get("skipped"))
print("files", c.get("files_copied"))
print("bytes", c.get("bytes"))
print("unchanged", r.get("source_unchanged"))
PY
[ "$code" = "200" ] && pass "import route answers 200" || bad "import route answers 200" "HTTP $code"
grep -q '^imported 9$' "$tmp/assert.txt" && pass "route imported 9 posts" || bad "route imported 9 posts" "$(tr '\n' ' ' < "$tmp/assert.txt")"
grep -q '^skipped 0$' "$tmp/assert.txt" && pass "route skipped none" || bad "route skipped none" "$(tr '\n' ' ' < "$tmp/assert.txt")"
grep -q '^unchanged True$' "$tmp/assert.txt" && pass "route reports the source unchanged" || bad "route reports the source unchanged" "$(tr '\n' ' ' < "$tmp/assert.txt")"
# Same numbers as the core test on a temp data dir: 30 files, 47,164,542 bytes.
grep -q '^files 30$' "$tmp/assert.txt" && pass "route copied 30 images" || bad "route copied 30 images" "$(tr '\n' ' ' < "$tmp/assert.txt")"
grep -q '^bytes 47164542$' "$tmp/assert.txt" && pass "route copied 47,164,542 bytes" || bad "route copied 47,164,542 bytes" "$(tr '\n' ' ' < "$tmp/assert.txt")"

ls "$tmp/data/recipes" 2>/dev/null | wc -l | tr -d ' ' > "$tmp/recipes"
[ "$(cat "$tmp/recipes")" = "9" ] && pass "9 recipes written" || bad "9 recipes written" "found $(cat "$tmp/recipes")"
[ -f "$tmp/data/imports/ledger.json" ] && pass "ledger written" || bad "ledger written"
[ -f "$tmp/data/pool/index.json" ] && pass "pool index written" || bad "pool index written"

code="$(curl -sS -m 120 -o "$tmp/again" -w '%{http_code}' -X POST "$url/api/test/trigger/import" \
  -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
  --data "{\"root\":\"$CHANNEL\",\"apply\":true}")"
again="$(python3 -I -c "import json;print(json.load(open('$tmp/again')).get('report',{}).get('counts',{}).get('imported'))" 2>/dev/null)"
[ "$code" = "200" ] && [ "$again" = "0" ] && pass "second route run copies nothing" \
  || bad "second route run copies nothing" "HTTP $code imported=$again"

snapshot > "$tmp/after.txt"
cmp -s "$tmp/before.txt" "$tmp/after.txt" && pass "Story's files are byte-identical after the run" \
  || bad "Story's files are byte-identical after the run" "source hashes differ"

echo "RESULT fails=$fails"
[ "$fails" -eq 0 ]
