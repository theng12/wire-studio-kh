#!/bin/bash
# Drives variant fan-out through the app's own route: one post, two targets, a
# chosen format preset for each, then reads the queue back. No live account is
# needed because the rows are scheduled into the future.
# Usage: fanout-route-proof.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${WIRE_BIN:-$ROOT/src-tauri/target/debug/wire-studio-kh}"
[ -x "$BIN" ] || { echo "FAIL binary missing: $BIN"; exit 1; }

fails=0
pass() { echo "PASS $1"; }
bad() { echo "FAIL $1${2:+ — $2}"; fails=$((fails+1)); }

tmp="$(mktemp -d /tmp/wire-fanout-route.XXXXXX)"
port="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
pid=""
cleanup() { [ -n "$pid" ] && kill -TERM "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; rm -rf "$tmp"; }
trap cleanup EXIT

# The installed app may already hold 8649; this proof never exercises the hold,
# and a test instance that cannot bind it logs one line and carries on.
if lsof -nP -iTCP:8649 -sTCP:LISTEN >/dev/null 2>&1; then
  echo "NOTE hold port 8649 is held by another app; fan-out does not need it"
fi

WIRE_STUDIO_KH_DATA_DIR="$tmp/data" "$BIN" --test-mode --KH_TEST_PORT "$port" > "$tmp/log" 2>&1 &
pid=$!
for _ in $(seq 1 50); do grep -q '^TEST_URL=' "$tmp/log" && break; sleep 0.2; done
grep -q '^TEST_URL=' "$tmp/log" && pass "test-mode server ready on $port" || { bad "test-mode server ready"; exit 1; }
url="http://127.0.0.1:$port"
token="$(python3 -I -c "import json;print(json.load(open('$tmp/data/app-secrets.json'))['operator_token'])" 2>/dev/null)"
[ -n "$token" ] && pass "operator token available" || { bad "operator token available"; exit 1; }

code="$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
  -H 'Content-Type: application/json' --data '{"text":"x","targets":["telegram:@c"]}')"
[ "$code" = "403" ] && pass "schedule refuses a caller without the operator credential" \
  || bad "schedule refuses a caller without the operator credential" "HTTP $code"

code="$(curl -sS -m 10 -o "$tmp/bad" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
  -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
  --data '{"text":"cross-platform preset","targets":["youtube:chan"],"presets":{"youtube:chan":"fb-1x1"}}')"
reason="$(python3 -I -c "import json;print(json.load(open('$tmp/bad')).get('error',''))" 2>/dev/null)"
[ "$code" = "400" ] && case "$reason" in *fb-1x1*) true;; *) false;; esac \
  && pass "a Facebook preset on YouTube is refused, naming it" \
  || bad "a Facebook preset on YouTube is refused, naming it" "HTTP $code error=$reason"

code="$(curl -sS -m 10 -o "$tmp/ok" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
  -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
  --data '{"text":"fan-out proof","targets":["youtube:chan","facebook_page:page","facebook_page:page"],"presets":{"youtube:chan":"yt-shorts"},"delay_s":3600}')"
ids="$(python3 -I -c "import json;print(len(json.load(open('$tmp/ok')).get('delivery_ids',[])))" 2>/dev/null)"
[ "$code" = "200" ] && [ "$ids" = "2" ] && pass "schedule answers one row per target and preset, a repeated target deduped" \
  || bad "schedule answers one row per target and preset, a repeated target deduped" "HTTP $code ids=$ids"

curl -sS -m 10 "$url/api/test/state" -o "$tmp/state"
python3 -I - "$tmp/state" <<'PY' > "$tmp/assert.txt" || true
import json, sys
rows = json.load(open(sys.argv[1])).get("deliveries", [])
print("rows", len(rows))
for row in sorted(rows, key=lambda r: (r.get("target") or "", r.get("preset") or "")):
    print("row", row.get("target"), row.get("preset") or "(none)", row.get("state") or row.get("status"))
PY
grep -q '^rows 2$' "$tmp/assert.txt" && pass "queue holds one row per target, the repeated target deduped" || bad "queue holds one row per target, the repeated target deduped" "$(tr '\n' ' ' < "$tmp/assert.txt")"
grep -q '^row facebook_page:page fb-1x1' "$tmp/assert.txt" && pass "Facebook row carries its default square preset" || bad "Facebook row carries its default square preset" "$(tr '\n' ' ' < "$tmp/assert.txt")"
grep -q '^row youtube:chan yt-shorts' "$tmp/assert.txt" && pass "YouTube row carries the chosen Shorts preset" || bad "YouTube row carries the chosen Shorts preset" "$(tr '\n' ' ' < "$tmp/assert.txt")"

echo "RESULT fails=$fails"
[ "$fails" -eq 0 ]
