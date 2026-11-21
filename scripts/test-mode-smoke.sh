#!/usr/bin/env bash
# Headless entry smoke: printed URL/token, temp data dir, guarded trigger, no window, clean SIGTERM.
set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${WIRE_BIN:-$root/src-tauri/target/debug/wire-studio-kh}"
[ -x "$bin" ] || bin="$root/src-tauri/target/release/wire-studio-kh"

fails=0
pass() { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s (observed %s)\n' "$1" "$2"; fails=$((fails + 1)); }
json_field() {
  python3 -I -c '
import json, sys
try:
    value = json.load(open(sys.argv[1]))
    for key in sys.argv[2].split("."):
        value = value[key]
    if isinstance(value, bool): print(str(value).lower())
    elif value is None: print("null")
    elif isinstance(value, (list, dict)): print(json.dumps(value, separators=(",", ":")))
    else: print(value)
except Exception:
    print("")
' "$1" "$2"
}

if [ ! -x "$bin" ]; then bad "binary present" "no executable at $bin"; exit 1; fi
pass "binary present"

tmp="$(mktemp -d /tmp/wire-test-smoke.XXXXXX)"
data="$tmp/data"
mkdir -p "$data"
log="$tmp/test.log"
token="wire-smoke-$$-$RANDOM"
port="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
url="http://127.0.0.1:$port"
pid=""

cleanup() {
  rc=$?
  if [ -n "$pid" ]; then
    kill -9 "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -rf "$tmp"
  exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# main.rs names the test data dir from TMPDIR + pid. Hold the wrapper pid steady until the
# symlink below is in place, so WIRE_STUDIO_KH_DATA_DIR is the directory actually used.
rm -f "$tmp/pidfile" "$tmp/go"
TMPDIR="$tmp" bash -c '
  echo $$ > "$1"
  while [ ! -f "$2" ]; do sleep 0.05; done
  shift 2
  exec "$@"
' _ "$tmp/pidfile" "$tmp/go" env TMPDIR="$tmp" WIRE_STUDIO_KH_DATA_DIR="$data" \
    WIRE_OPERATOR_TOKEN="$token" KH_TEST_PORT="$port" "$bin" --test-mode --KH_TEST_PORT "$port" \
    >"$log" 2>&1 &
for tries in $(seq 1 100); do [ -s "$tmp/pidfile" ] && break; sleep 0.05; done
pid="$(cat "$tmp/pidfile" 2>/dev/null || true)"
if [ -z "$pid" ]; then bad "test-mode launch" "no pid written"; exit 1; fi
ln -sfn "$data" "$tmp/wire-studio-kh-test-$pid"
: > "$tmp/go"

ready=0
for tries in $(seq 1 100); do
  if grep -q '^TEST_URL=' "$log" 2>/dev/null; then ready=1; break; fi
  kill -0 "$pid" 2>/dev/null || break
  sleep 0.1
done
if [ "$ready" -eq 1 ]; then pass "TEST_URL printed"; else bad "TEST_URL printed" "absent; log at $log"; exit 1; fi

printed_url="$(grep -m1 '^TEST_URL=' "$log")"
if [ "$printed_url" = "TEST_URL=$url" ]; then pass "TEST_URL uses requested loopback port"; else bad "TEST_URL uses requested loopback port" "$printed_url (expected TEST_URL=$url)"; fi
printed_token="$(grep -m1 '^TEST_TOKEN=' "$log" | cut -d= -f2-)"
if [ "$printed_token" = "$token" ]; then pass "TEST_TOKEN printed"; else bad "TEST_TOKEN printed" "$printed_token (expected $token)"; fi
printed_dir="$(grep -m1 '^TEST_DATA_DIR=' "$log" || true)"
case "$printed_dir" in
  "TEST_DATA_DIR=$tmp"*) pass "TEST_DATA_DIR is inside the fresh temp dir" ;;
  *) bad "TEST_DATA_DIR is inside the fresh temp dir" "$printed_dir (expected under $tmp)" ;;
esac

http="$(curl -sS -m 5 -o "$tmp/state" -w '%{http_code}' "$url/api/test/state" || true)"
if [ "$http" = "200" ]; then pass "GET /api/test/state answers 200"; else bad "GET /api/test/state answers 200" "HTTP $http"; fi
state_keys="$(python3 -I -c '
import json, sys
try:
    value = json.load(open(sys.argv[1]))
    missing = [key for key in ("paused", "in_flight", "deliveries") if key not in value]
    print("OK" if not missing else "missing " + ",".join(missing) + "; observed keys " + ",".join(sorted(value)))
except Exception as error:
    print("invalid JSON: " + str(error))
' "$tmp/state")"
if [ "$state_keys" = "OK" ]; then pass "state JSON has paused, in_flight, deliveries"; else bad "state JSON has paused, in_flight, deliveries" "$state_keys"; fi

http="$(curl -sS -m 5 -o "$tmp/guard" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
  -H 'Content-Type: application/json' --data '{"text":"x","targets":["telegram:TESTCHAT"],"delay_s":0}' || true)"
guard_error="$(json_field "$tmp/guard" error)"
if [ "$http" = "403" ] && [ "$guard_error" = "guard: operator credential required" ]; then
  pass "trigger without operator header is 403 naming the guard"
else
  bad "trigger without operator header is 403 naming the guard" "HTTP $http error=$guard_error"
fi

http="$(curl -sS -m 5 -o "$tmp/trigger" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
  -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
  --data '{"text":"smoke","targets":["telegram:TESTCHAT"],"delay_s":0}' || true)"
trigger_id="$(json_field "$tmp/trigger" post_id)"
if [ "$http" = "200" ] && [ -n "$trigger_id" ]; then
  pass "trigger with x-wire-operator answers 200"
else
  bad "trigger with x-wire-operator answers 200" "HTTP $http body=$(cat "$tmp/trigger" 2>/dev/null)"
fi

# A GUI app maps WindowServer; the headless core must never connect to it.
# This reads the process's own open files — no AppleScript, no clicks, no focus.
if lsof -p "$pid" 2>/dev/null | grep -q 'WindowServer'; then
  bad "headless process never connects to WindowServer" "WindowServer file open"
else
  pass "headless process never connects to WindowServer"
fi

term_rc="none"
kill -TERM "$pid" 2>/dev/null || term_rc="signal failed"
if [ "$term_rc" = "none" ]; then
  for tries in $(seq 1 50); do
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null; term_rc=$?
      break
    fi
    sleep 0.1
  done
fi
if [ "$term_rc" = "0" ]; then
  pass "SIGTERM exits 0"
else
  bad "SIGTERM exits 0" "$term_rc (expected exit 0 within 5s)"
fi
pid=""

port_state="$(python3 -I -c '
import socket, sys
sock = socket.socket()
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    sock.bind(("127.0.0.1", int(sys.argv[1])))
    print("free")
except OSError as error:
    print("busy: " + str(error))
finally:
    sock.close()
' "$port")"
if [ "$port_state" = "free" ]; then pass "test port released after exit"; else bad "test port released after exit" "$port_state on $port"; fi

printf '%s\n' "RESULT fails=$fails"
[ "$fails" -eq 0 ] && exit 0
exit 1
