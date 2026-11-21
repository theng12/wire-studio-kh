#!/usr/bin/env bash
# Live-work update hold proof: hold with work queued, kill -9 mid-hold, restart, dispatch resumes.
# Uses curl only for HTTP; python3 is used for JSON and a free-port lookup.
set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${WIRE_BIN:-$root/src-tauri/target/debug/wire-studio-kh}"
[ -x "$bin" ] || bin="$root/src-tauri/target/release/wire-studio-kh"

fails=0
pass() { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fails=$((fails + 1)); }
json() {
  python3 -c '
import json, sys
try:
    value = json.load(open(sys.argv[1]))
    for key in sys.argv[2].split("."):
        value = value[key]
    if value is None: print("null")
    elif isinstance(value, (list, dict)): print(json.dumps(value))
    else: print(value)
except Exception:
    print("")
' "$1" "$2"
}
row_state() {
  python3 -c '
import json, sys
rows = json.load(open(sys.argv[1])).get("deliveries") or []
wanted = sys.argv[2]
for row in rows:
    if str(row.get("id")) == wanted:
        print(row.get("state", ""))
        break
' "$1" "$2"
}

if [ ! -x "$bin" ]; then
  bad "binary missing: $bin (build first)"
  exit 1
fi
pass "binary present"

tmp="$(mktemp -d /tmp/wire-hold-proof.XXXXXX)"
data="$tmp/data"
mkdir -p "$data"
log="$tmp/test.log"
token="wire-proof-$$-$RANDOM"
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
url="http://127.0.0.1:$port"
hold_url="http://127.0.0.1:8649/api/maintenance/hold?timeout_s=1&pause_s=120"
pid=""
trap 'kill -9 "$pid" 2>/dev/null || true; rm -rf "$tmp"' EXIT

# main.rs derives the test data dir from TMPDIR + pid. Redirect that one directory to our
# persistent data dir before exec, so a killed process and its restart share the same queue.
launch() {
  rm -f "$tmp/go" "$tmp/pidfile" "$log"
  TMPDIR="$tmp" bash -c '
    echo $$ > "$1"
    while [ ! -f "$2" ]; do sleep 0.05; done
    shift 2
    exec "$@"
  ' _ "$tmp/pidfile" "$tmp/go" env TMPDIR="$tmp" WIRE_STUDIO_KH_DATA_DIR="$data" \
      WIRE_OPERATOR_TOKEN="$token" KH_TEST_PORT="$port" "$bin" --test-mode --KH_TEST_PORT "$port" \
      >"$log" 2>&1 &
  local tries
  for tries in $(seq 1 100); do
    [ -s "$tmp/pidfile" ] && break
    sleep 0.05
  done
  pid="$(cat "$tmp/pidfile" 2>/dev/null || true)"
  [ -n "$pid" ] || { bad "launch wrote no pid"; return 1; }
  ln -sfn "$data" "$tmp/wire-studio-kh-test-$pid"
  : > "$tmp/go"
  for tries in $(seq 1 100); do
    curl -fsS -m 1 "$url/api/test/state" >/dev/null 2>&1 && return 0
    kill -0 "$pid" 2>/dev/null || return 1
    sleep 0.1
  done
  return 1
}

if launch; then
  pass "test-mode server ready on $port"
else
  bad "test-mode server did not start (see $log)"
  exit 1
fi

case "$(grep -m1 '^TEST_DATA_DIR=' "$log" || true)" in
  "TEST_DATA_DIR=$tmp"*) pass "temp data dir used" ;;
  *) bad "TEST_DATA_DIR does not point into the temp dir" ;;
esac
[ -f "$data/wire.db" ] && pass "queue database in temp data dir" || bad "wire.db missing in temp data dir"

code="$(curl -sS -m 5 -o "$tmp/r" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
  -H 'Content-Type: application/json' \
  --data '{"text":"hold proof","targets":["telegram:CHAT"],"delay_s":0}' || true)"
if [ "$code" = "403" ] && [ "$(json "$tmp/r" error)" = "guard: operator credential required" ]; then
  pass "schedule without operator token refused"
else
  bad "schedule without operator token: HTTP $code"
fi

code="$(curl -sS -m 5 -o "$tmp/r" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
  -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
  --data '{"text":"hold proof","targets":["telegram:CHAT"],"delay_s":0}' || true)"
post_id="$(json "$tmp/r" post_id)"
delivery_id="$(python3 -c '
import json, sys
ids = json.load(open(sys.argv[1])).get("delivery_ids") or []
print(ids[0] if ids else "")
' "$tmp/r" 2>/dev/null || true)"
if [ "$code" = "200" ] && [ -n "$post_id" ] && [ -n "$delivery_id" ]; then
  pass "queued live work: post $post_id delivery $delivery_id"
else
  bad "schedule returned HTTP $code: $(cat "$tmp/r")"
  exit 1
fi

hold_token=""
if [ -f "$HOME/Library/KHUpdater/hold-token" ]; then
  hold_token="$(tr -d '[:space:]' < "$HOME/Library/KHUpdater/hold-token")"
fi
origin_code="$(curl -sS -m 5 -o "$tmp/origin" -w '%{http_code}' -X POST "$hold_url" \
  -H 'Origin: https://evil.example' ${hold_token:+-H "x-hold-token: $hold_token"} || true)"
[ "$origin_code" = "403" ] && pass "foreign Origin refused" || bad "foreign Origin: HTTP $origin_code"

if [ -n "$hold_token" ]; then
  code="$(curl -sS -m 35 -o "$tmp/hold" -w '%{http_code}' -X POST "$hold_url" \
    -H "x-hold-token: $hold_token" || true)"
else
  code="$(curl -sS -m 35 -o "$tmp/hold" -w '%{http_code}' -X POST "$hold_url" || true)"
fi
safe="$(json "$tmp/hold" safe)"
inflight="$(json "$tmp/hold" in_flight)"
if [ "$code" = "200" ] && [ "$(json "$tmp/hold" dispatch_paused_s)" = "120" ]; then
  pass "hold reply HTTP 200, pause 120s"
else
  bad "hold reply HTTP $code: $(cat "$tmp/hold" 2>/dev/null)"
fi
if [ "$safe" = "true" ] && [ "$inflight" = "0" ]; then
  pass "hold reports settled in_flight=0"
elif [ "$safe" = "false" ] && { [ "$inflight" != "0" ] || [ "$(json "$tmp/hold" unsettled)" != "[]" ]; }; then
  pass "hold reports unsettled work: $(json "$tmp/hold" unsettled)"
else
  bad "hold reply safe=$safe in_flight=$inflight"
fi

kill -9 "$pid" 2>/dev/null || true
wait "$pid" 2>/dev/null || true
pass "killed -9 while hold was active"

if launch; then
  pass "restarted with the same queue"
else
  bad "restart did not come up (see $log)"
  exit 1
fi

curl -fsS -m 5 "$url/api/test/state" > "$tmp/state" || true
state="$(row_state "$tmp/state" "$delivery_id")"
case "$state" in
  pending|queued) pass "queued row survived kill -9 as $state" ;;
  *) bad "queued row after restart: ${state:-missing}" ;;
esac

resumed=""
for tries in $(seq 1 150); do
  curl -fsS -m 2 "$url/api/test/state" > "$tmp/state" || break
  state="$(row_state "$tmp/state" "$delivery_id")"
  case "$state" in
    ""|pending|queued) ;;
    *) resumed="$state"; break ;;
  esac
  sleep 0.1
done
[ -n "$resumed" ] && pass "dispatch resumed without DELETE (row $resumed)" || bad "dispatch did not resume within 15s"

if [ -n "$hold_token" ]; then
  code="$(curl -sS -m 5 -o "$tmp/resume" -w '%{http_code}' -X DELETE "$hold_url" \
    -H "x-hold-token: $hold_token" || true)"
else
  code="$(curl -sS -m 5 -o "$tmp/resume" -w '%{http_code}' -X DELETE "$hold_url" || true)"
fi
if [ "$code" = "200" ] && [ "$(json "$tmp/resume" resumed)" = "true" ]; then
  pass "DELETE resumes dispatch"
else
  bad "DELETE returned HTTP $code: $(cat "$tmp/resume" 2>/dev/null)"
fi

printf '%s\n' "RESULT fails=$fails"
[ "$fails" -eq 0 ]
