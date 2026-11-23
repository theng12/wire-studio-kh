#!/usr/bin/env bash
# Wire <-> Gateway connect proof, against a real Gateway, hidden, with temp dirs for both.
#
#   bash scripts/gateway-connect-proof.sh
#
# Proves, with no window and nothing written to the owner's data:
#   1 a wrong key is refused in Gateway's own words, and never saved as connected
#   2 the right key reads the live catalogue and says how many models it publishes
#   3 an empty catalogue is healthy ("no models published yet"), not a failure
#   4 the key is never printed or stored in the state; only its last four are shown
#   5 rotating the key at Gateway (a revocation) turns a saved connection into "failed"
#   6 reconnecting with the new key works
#   7 a Gateway that is not running says "Nothing is listening at …"
#   8 Choose… really lists folders, and an unreadable folder says why
#
# Needs a Python with Gateway's dependencies. Set WIRE_GATEWAY_PY, or let it build one with uv
# at $TMPDIR/wire-gateway-venv. GATEWAY_ENDPOINT_FILE always points inside the temp dir so the
# probe Gateway never rewrites ~/.kh/gateway-endpoint.json (the file Relay reads).
set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

bin="${WIRE_BIN:-$root/src-tauri/target/debug/wire-studio-kh}"
[ -x "$bin" ] || bin="$root/src-tauri/target/release/wire-studio-kh"
gateway_repo="${GATEWAY_REPO:-$HOME/Developer/_graphs/gateway}"
py="${WIRE_GATEWAY_PY:-${TMPDIR:-/tmp}/wire-gateway-venv/bin/python}"

fails=0
pass() { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s (observed %s)\n' "$1" "$2"; fails=$((fails + 1)); }
field() {
  python3 -I -c '
import json, sys
try:
    value = json.load(open(sys.argv[1]))
    for key in sys.argv[2].split("."):
        value = value[key]
    print(json.dumps(value, separators=(",", ":")) if isinstance(value, (list, dict)) else
          ("" if value is None else str(value).lower() if isinstance(value, bool) else str(value)))
except Exception:
    print("")
' "$1" "$2"
}
free_port() {
  python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

[ -x "$bin" ] || { bad "binary present" "no executable at $bin"; exit 1; }
pass "binary present"
[ -d "$gateway_repo/gateway" ] || { bad "Gateway checkout present" "none at $gateway_repo"; exit 1; }
pass "Gateway checkout present"

tmp="$(mktemp -d /tmp/wire-gateway-proof.XXXXXX)"
gw_pid=""
wire_pid=""

cleanup() {
  rc=$?
  [ -n "$wire_pid" ] && kill -9 "$wire_pid" 2>/dev/null
  [ -n "$gw_pid" ] && kill -9 "$gw_pid" 2>/dev/null
  wait 2>/dev/null
  rm -rf "$tmp"
  exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ---------- a Python that can run Gateway ----------
if [ ! -x "$py" ] || ! "$py" -c 'import fastapi, uvicorn, httpx' 2>/dev/null; then
  uv_bin="$(command -v uv || true)"
  [ -n "$uv_bin" ] || { bad "Gateway runs here" "no WIRE_GATEWAY_PY and no uv on PATH"; exit 1; }
  uv venv --python 3.12 "$(dirname "$(dirname "$py")")" >/dev/null 2>&1
  VIRTUAL_ENV="$(dirname "$(dirname "$py")")" "$uv_bin" pip install -q -r "$gateway_repo/requirements.lock" >/dev/null 2>&1
  "$py" -c 'import fastapi, uvicorn, httpx' 2>/dev/null || { bad "Gateway runs here" "dependencies did not install into $py"; exit 1; }
fi

# ---------- a real Gateway, isolated in the temp dir ----------
# The check M001's issue row asks for: the live endpoint file Relay reads must be byte-identical
# afterwards (or still absent). A test run never edits another app's live config.
live_endpoint="$HOME/.kh/gateway-endpoint.json"
live_before="absent"
[ -e "$live_endpoint" ] && live_before="$(shasum -a 256 "$live_endpoint" | awk '{print $1}')"

gw_port="$(free_port)"
gw_key="wire-proof-one-$RANDOM"
gw_dir="$tmp/gateway"
mkdir -p "$gw_dir"
write_env() {
  printf 'GATEWAY_API_KEY=%s\nGATEWAY_HOST=127.0.0.1\nGATEWAY_PORT=%s\n' "$1" "$gw_port" > "$gw_dir/.env"
}
write_env "$gw_key"
cd "$gateway_repo" || exit 1
GATEWAY_APP_SUPPORT="$gw_dir" \
GATEWAY_DATA="$gw_dir/data" \
GATEWAY_ENDPOINT_FILE="$gw_dir/endpoint.json" \
GATEWAY_ENV_FILE="$gw_dir/.env" \
  "$py" -m uvicorn gateway.api:app --host 127.0.0.1 --port "$gw_port" > "$tmp/gateway.log" 2>&1 &
gw_pid=$!
gw_up=0
for _ in $(seq 1 120); do
  if curl -fsS -m 2 "http://127.0.0.1:$gw_port/health" >/dev/null 2>&1; then gw_up=1; break; fi
  kill -0 "$gw_pid" 2>/dev/null || break
  sleep 0.5
done
if [ "$gw_up" = "1" ]; then pass "local Gateway answers /health"; else
  bad "local Gateway answers /health" "not up; log tail: $(tail -3 "$tmp/gateway.log" | tr '\n' ' ')"; exit 1
fi
live_after="absent"
[ -e "$live_endpoint" ] && live_after="$(shasum -a 256 "$live_endpoint" | awk '{print $1}')"
if [ "$live_before" = "$live_after" ]; then
  pass "the live ~/.kh/gateway-endpoint.json is unchanged ($live_after)"
else
  bad "the live ~/.kh/gateway-endpoint.json is unchanged" "was $live_before, now $live_after — GATEWAY_ENDPOINT_FILE is not pinned"
fi

# ---------- Wire, hidden, with its own data dir ----------
wire_port="$(free_port)"
wire_url="http://127.0.0.1:$wire_port"
wire_token="wire-gateway-proof-$$-$RANDOM"
data="$tmp/wire-data"
mkdir -p "$data"
TMPDIR="$tmp" WIRE_STUDIO_KH_DATA_DIR="$data" WIRE_OPERATOR_TOKEN="$wire_token" \
  "$bin" --test-mode --KH_TEST_PORT "$wire_port" > "$tmp/wire.log" 2>&1 &
wire_pid=$!
for _ in $(seq 1 120); do
  grep -q '^TEST_DATA_DIR=' "$tmp/wire.log" 2>/dev/null && break
  kill -0 "$wire_pid" 2>/dev/null || break
  sleep 0.1
done
grep -q '^TEST_DATA_DIR=' "$tmp/wire.log" 2>/dev/null || { bad "Wire test-mode started" "no TEST_DATA_DIR; $(tail -3 "$tmp/wire.log" | tr '\n' ' ')"; exit 1; }
pass "Wire test-mode started"

trigger() { # trigger <route> <json> <outfile> -> http code
  curl -sS -m 15 -o "$3" -w '%{http_code}' -X POST "$wire_url/api/test/trigger/$1" \
    -H "x-wire-operator: $wire_token" -H 'Content-Type: application/json' --data "$2" || true
}
gw_json() { printf '{"url":"http://127.0.0.1:%s","key":"%s"}' "$1" "$2"; }

# 1 — a wrong key is refused in Gateway's own words.
code="$(trigger gateway-save "$(gw_json "$gw_port" "not-the-key-$RANDOM")" "$tmp/bad")"
reason="$(field "$tmp/bad" reason)"
state="$(field "$tmp/bad" connection_state)"
if [ "$reason" = "Invalid API key." ] && [ "$state" = "failed" ]; then
  pass "a wrong key is refused with Gateway's own sentence"
else
  bad "a wrong key is refused with Gateway's own sentence" "HTTP $code state=$state reason=$reason"
fi

# 2 — the right key reads the live catalogue. A bare Gateway publishes none: that is healthy.
code="$(trigger gateway-save "$(gw_json "$gw_port" "$gw_key")" "$tmp/good")"
reason="$(field "$tmp/good" reason)"
if [ "$code" = "200" ] && [ "$(field "$tmp/good" ok)" = "true" ] && [ "$(field "$tmp/good" connection_state)" = "ready" ]; then
  pass "the right key connects"
else
  bad "the right key connects" "HTTP $code body=$(cat "$tmp/good" 2>/dev/null)"
fi

# 3 — an empty catalogue says so, and is never an error or a silent zero.
if [ "$reason" = "connected · no models published yet" ]; then
  pass "an empty catalogue reads 'no models published yet'"
else
  bad "an empty catalogue reads 'no models published yet'" "$reason"
fi

# 4 — the key never reaches the state or a log; only its last four are shown.
curl -sS -m 5 -o "$tmp/state" "$wire_url/api/test/state" >/dev/null 2>&1
if grep -qF "$gw_key" "$tmp/state" "$tmp/wire.log" "$tmp/good" 2>/dev/null; then
  bad "the key is never stored in the state" "the key text appears in state/log/answer"
else
  pass "the key is never stored in the state"
fi
tail4="$(printf '%s' "$gw_key" | tail -c 4)"
if [ "$(field "$tmp/state" gateway.key_masked)" = "…$tail4" ]; then
  pass "state shows only the last four of the key"
else
  bad "state shows only the last four of the key" "$(field "$tmp/state" gateway.key_masked)"
fi
if [ "$(field "$tmp/state" gateway.connection_state)" = "ready" ]; then
  pass "state reports the verified connection"
else
  bad "state reports the verified connection" "$(field "$tmp/state" gateway.connection_state)"
fi
# The saved key lives in the 0600 file, never in settings.json or a log.
mode="$(stat -f '%Lp' "$data/app-secrets.json" 2>/dev/null)"
if [ "$mode" = "600" ] && ! grep -qF "$gw_key" "$data/settings.json" 2>/dev/null; then
  pass "the key sits in the 0600 secrets file, not settings.json"
else
  bad "the key sits in the 0600 secrets file, not settings.json" "mode=$mode"
fi

# 5 — rotating the key at Gateway revokes the old one; Check connection must stop saying connected.
sleep 1
rotated="rotated-$RANDOM-longer"
write_env "$rotated"
reason_old="$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $gw_key" "http://127.0.0.1:$gw_port/v1/models" || true)"
[ "$reason_old" = "403" ] && pass "Gateway now refuses the old key (403)" \
  || bad "Gateway now refuses the old key (403)" "HTTP $reason_old"
# Both fields blank = Check connection: it reuses what is saved, exactly like the button.
code="$(trigger gateway-save '{"url":"","key":""}' "$tmp/revoked")"
reason="$(field "$tmp/revoked" reason)"
if [ "$(field "$tmp/revoked" connection_state)" = "failed" ] && [ "$reason" = "Invalid API key." ]; then
  pass "a revoked key turns the saved connection into 'failed'"
else
  bad "a revoked key turns the saved connection into 'failed'" "HTTP $code state=$(field "$tmp/revoked" connection_state) reason=$reason"
fi
curl -sS -m 5 -o "$tmp/state2" "$wire_url/api/test/state" >/dev/null 2>&1
if [ "$(field "$tmp/state2" gateway.connection_state)" = "failed" ]; then
  pass "the state no longer reads connected after a revocation"
else
  bad "the state no longer reads connected after a revocation" "$(field "$tmp/state2" gateway.connection_state)"
fi

# 6 — reconnecting with the new key works.
trigger gateway-save "$(gw_json "$gw_port" "$rotated")" "$tmp/reconnected" >/dev/null
if [ "$(field "$tmp/reconnected" connection_state)" = "ready" ]; then
  pass "reconnecting with the rotated key works"
else
  bad "reconnecting with the rotated key works" "$(field "$tmp/reconnected" reason)"
fi

# 7 — a Gateway that is not running says so in plain words.
dead_port="$(free_port)"
trigger gateway-save "$(gw_json "$dead_port" "any-key-value")" "$tmp/dead" >/dev/null
dead_reason="$(field "$tmp/dead" reason)"
if [ "$dead_reason" = "Nothing is listening at http://127.0.0.1:$dead_port." ]; then
  pass "a Gateway that is not running says 'Nothing is listening at …'"
else
  bad "a Gateway that is not running says 'Nothing is listening at …'" "$dead_reason"
fi

# 8 — Choose… really lists folders, and a folder that cannot be read says why.
mkdir -p "$tmp/browse/one" "$tmp/browse/two"
mkdir -p "$tmp/unreadable" && chmod 000 "$tmp/unreadable"
code="$(trigger list-dirs "{\"path\":\"$tmp/browse\"}" "$tmp/dirs")"
names="$(field "$tmp/dirs" dirs)"
if [ "$code" = "200" ] && printf '%s' "$names" | grep -q '"one"' && printf '%s' "$names" | grep -q '"two"'; then
  pass "Choose… lists real folders"
else
  bad "Choose… lists real folders" "HTTP $code body=$(cat "$tmp/dirs" 2>/dev/null)"
fi
code="$(trigger list-dirs "{\"path\":\"$tmp/unreadable\"}" "$tmp/denied")"
if [ "$code" = "400" ] && [ -n "$(field "$tmp/denied" error)" ]; then
  pass "an unreadable folder says why"
else
  bad "an unreadable folder says why" "HTTP $code body=$(cat "$tmp/denied" 2>/dev/null)"
fi
chmod 700 "$tmp/unreadable"

# Hidden, always: no WindowServer connection, no window.
if lsof -p "$wire_pid" 2>/dev/null | grep -q 'WindowServer'; then
  bad "no window and no desktop" "WindowServer connection open"
else
  pass "no window and no desktop"
fi

# Clean exit on SIGTERM.
kill -TERM "$wire_pid" 2>/dev/null
exited=""
for _ in $(seq 1 50); do
  if ! kill -0 "$wire_pid" 2>/dev/null; then wait "$wire_pid" 2>/dev/null; exited=$?; break; fi
  sleep 0.1
done
wire_pid=""
[ "$exited" = "0" ] && pass "SIGTERM exits 0" || bad "SIGTERM exits 0" "rc=${exited:-still running}"

kill -TERM "$gw_pid" 2>/dev/null; wait "$gw_pid" 2>/dev/null; gw_pid=""

if [ "$fails" -eq 0 ]; then
  printf 'GATEWAY CONNECT PROOF PASS\n'
  exit 0
fi
printf 'GATEWAY CONNECT PROOF FAILED (%s checks)\n' "$fails"
exit 1
