#!/usr/bin/env bash
# Headless entry smoke: printed URL/token, temp data dir, no window, clean SIGTERM.
set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${WIRE_BIN:-$root/src-tauri/target/debug/wire-studio-kh}"
[ -x "$bin" ] || bin="$root/src-tauri/target/release/wire-studio-kh"

fails=0
pass() { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fails=$((fails + 1)); }

if [ ! -x "$bin" ]; then
  bad "binary missing: $bin (build first)"
  exit 1
fi
pass "binary present"

tmp="$(mktemp -d /tmp/wire-test-smoke.XXXXXX)"
log="$tmp/test.log"
token="wire-smoke-$$-$RANDOM"
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
url="http://127.0.0.1:$port"
pid=""
trap 'kill -9 "$pid" 2>/dev/null || true; rm -rf "$tmp"' EXIT

TMPDIR="$tmp" WIRE_STUDIO_KH_DATA_DIR="$tmp/data" WIRE_OPERATOR_TOKEN="$token" KH_TEST_PORT="$port" \
  "$bin" --test-mode --KH_TEST_PORT "$port" >"$log" 2>&1 &
pid=$!

ready=0
for tries in $(seq 1 100); do
  if grep -q '^TEST_URL=' "$log" 2>/dev/null; then ready=1; break; fi
  kill -0 "$pid" 2>/dev/null || break
  sleep 0.1
done
[ "$ready" -eq 1 ] && pass "TEST_URL printed" || { bad "TEST_URL not printed (see $log)"; exit 1; }

printed_url="$(grep -m1 '^TEST_URL=' "$log")"
[ "$printed_url" = "TEST_URL=$url" ] && pass "TEST_URL uses the requested loopback port" || bad "TEST_URL was $printed_url"
printed_token="$(grep -m1 '^TEST_TOKEN=' "$log" | cut -d= -f2-)"
[ "$printed_token" = "$token" ] && pass "TEST_TOKEN printed" || bad "TEST_TOKEN was $printed_token"

case "$(grep -m1 '^TEST_DATA_DIR=' "$log" || true)" in
  "TEST_DATA_DIR=$tmp"*) pass "temp data dir used" ;;
  *) bad "TEST_DATA_DIR does not point into the temp dir" ;;
esac

http="$(curl -sS -m 5 -o "$tmp/state" -w '%{http_code}' "$url/api/test/state" || true)"
[ "$http" = "200" ] && pass "GET /api/test/state is open" || bad "GET state: HTTP $http"
http="$(curl -sS -m 5 -o "$tmp/guard" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
  -H 'Content-Type: application/json' --data '{"text":"x","targets":["telegram:CHAT"],"delay_s":0}' || true)"
[ "$http" = "403" ] && grep -q 'guard: operator credential required' "$tmp/guard" \
  && pass "act route without credential refused" || bad "unauthenticated schedule: HTTP $http"

# A hidden headless process must own no windows. System Events may lack permission on a CI box;
# then the check is reported as not checked rather than silently called a pass.
window_check=0
for proc in "Wire Studio KH" "wire-studio-kh"; do
  count="$(osascript -e "tell application \"System Events\" to count windows of process \"$proc\"" 2>&1)"
  rc=$?
  if [ "$rc" -eq 0 ]; then
    if [ "$count" = "0" ]; then window_check=1; else window_check=2; fi
    break
  fi
done
if [ "$window_check" -eq 1 ]; then
  pass "no windows owned by the headless process"
elif [ "$window_check" -eq 2 ]; then
  bad "headless process owns $count window(s)"
else
  printf 'NOT CHECKED window count (System Events refused the query)\n'
fi

kill -TERM "$pid" 2>/dev/null || true
exited=0
for tries in $(seq 1 50); do
  if ! kill -0 "$pid" 2>/dev/null; then exited=1; break; fi
  sleep 0.1
done
if [ "$exited" -eq 1 ]; then
  pass "SIGTERM exits cleanly"
else
  bad "SIGTERM did not exit within 5s"
  kill -9 "$pid" 2>/dev/null || true
fi
curl -fsS -m 1 "$url/api/test/state" >/dev/null 2>&1 && bad "port still answering after exit" || pass "port released on exit"

printf '%s\n' "RESULT fails=$fails"
[ "$fails" -eq 0 ]
