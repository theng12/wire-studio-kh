#!/bin/bash
# Drives the in-app Telegram connect flow through the app's own routes, with a
# deliberately invalid token: every step must refuse in Telegram's own words and
# save nothing. The success path needs a real bot and is the owner's walkthrough.
# Usage: telegram-connect-proof.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

BIN="${WIRE_BIN:-$ROOT/src-tauri/target/debug/wire-studio-kh}"
[ -x "$BIN" ] || { echo "FAIL binary missing: $BIN"; exit 1; }

fails=0
pass() { echo "PASS $1"; }
bad() { echo "FAIL $1${2:+ — $2}"; fails=$((fails+1)); }

tmp="$(mktemp -d /tmp/wire-tg-connect.XXXXXX)"
port="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
pid=""
cleanup() { [ -n "$pid" ] && kill -TERM "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; rm -rf "$tmp"; }
trap cleanup EXIT

WIRE_STUDIO_KH_DATA_DIR="$tmp/data" "$BIN" --test-mode --KH_TEST_PORT "$port" > "$tmp/log" 2>&1 &
pid=$!
for _ in $(seq 1 100); do grep -q '^TEST_DATA_DIR=' "$tmp/log" 2>/dev/null && break; sleep 0.1; done
grep -q '^TEST_DATA_DIR=' "$tmp/log" && pass "test-mode server ready on $port" || { bad "test-mode server ready"; exit 1; }
url="http://127.0.0.1:$port"
token="$(python3 -I -c "import json;print(json.load(open('$tmp/data/app-secrets.json'))['operator_token'])" 2>/dev/null)"
[ -n "$token" ] && pass "operator token available" || { bad "operator token available"; exit 1; }

post() { curl -sS -m 30 -o "$tmp/reply" -w '%{http_code}' -X POST "$url/api/test/trigger/$1" \
  -H "x-wire-operator: $token" -H 'Content-Type: application/json' --data "$2"; }
field() { python3 -I -c "import json;d=json.load(open('$tmp/reply'));print(d.get('$1',''))" 2>/dev/null; }

code="$(post telegram-connected-as '{"token":"123456:deliberately-invalid"}')"
[ "$code" = "200" ] && [ "$(field ok)" = "False" ] && grep -qi "unauthorized" "$tmp/reply" \
  && pass "Connect refuses a bad token in Telegram's words" \
  || bad "Connect refuses a bad token in Telegram's words" "HTTP $code $(cat "$tmp/reply")"

code="$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -X POST "$url/api/test/trigger/telegram-discover" \
  -H 'Content-Type: application/json' --data '{}')"
[ "$code" = "403" ] && pass "Find my channel needs the operator credential" \
  || bad "Find my channel needs the operator credential" "HTTP $code"

code="$(post telegram-use-channel '{"text":"@no_such_channel_for_wire"}')"
[ "$code" = "200" ] && [ "$(field ok)" = "False" ] \
  && pass "Use this channel refuses while the bot is not connected" \
  || bad "Use this channel refuses while the bot is not connected" "HTTP $code $(cat "$tmp/reply")"

# A saved-but-unverified token is enough for the parsing rules to answer first.
code="$(post save-telegram '{"token":"123456:deliberately-invalid","chat":"-1000000000001"}')"
[ "$code" = "200" ] && pass "a bad token is still saved, so the flow can be explained" \
  || bad "a bad token is still saved" "HTTP $code $(cat "$tmp/reply")"

code="$(post telegram-use-channel '{"text":"https://t.me/+invitehashonly"}')"
[ "$code" = "200" ] && grep -qi "invite link" "$tmp/reply" \
  && pass "an invite link gets the plain explanation, not a saved channel" \
  || bad "an invite link gets the plain explanation" "$(cat "$tmp/reply")"

code="$(post telegram-use-channel '{"text":"@no_such_channel_for_wire"}')"
[ "$code" = "200" ] && [ "$(field ok)" = "False" ] && grep -qi "unauthorized" "$tmp/reply" \
  && pass "a name is resolved through Telegram and refused in its words" \
  || bad "a name is resolved through Telegram and refused in its words" "$(cat "$tmp/reply")"

saved="$(python3 -I -c "import json;print(json.load(open('$tmp/data/settings.json')).get('telegram_chat',''))" 2>/dev/null)"
[ "$saved" = "-1000000000001" ] && pass "no refusal changed the saved channel" || bad "no refusal changed the saved channel" "saved=$saved"

echo "RESULT fails=$fails"
[ "$fails" -eq 0 ]
