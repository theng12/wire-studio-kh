#!/usr/bin/env bash
# Live-work update hold proof: hold with work queued, kill -9 mid-hold, restart, expiry resumes.
# curl only for HTTP; python3 -I for JSON. --expiry-only runs just the self-expiry proof.
set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

bin="${WIRE_BIN:-$root/src-tauri/target/debug/wire-studio-kh}"
[ -x "$bin" ] || bin="$root/src-tauri/target/release/wire-studio-kh"

fails=0
expiry_only=0
case "${1:-}" in
  --expiry-only) expiry_only=1 ;;
  "") ;;
  *) printf 'FAIL unknown argument (observed %s)\n' "$1"; exit 2 ;;
esac

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
row_state() {
  python3 -I -c '
import json, sys
try:
    rows = json.load(open(sys.argv[1])).get("deliveries") or []
except Exception:
    print("invalid state JSON")
    raise SystemExit
for row in rows:
    if str(row.get("id")) == sys.argv[2]:
        print(row.get("state", ""))
        break
' "$1" "$2"
}
contract_keys() {
  python3 -I -c '
import json, sys
try:
    value = json.load(open(sys.argv[1]))
except Exception as error:
    print("invalid JSON: " + str(error))
    raise SystemExit
problems = []
for key, kind in (("safe", bool), ("unsettled", list), ("dispatch_paused_s", (int, float)), ("in_flight", (int, float))):
    if key not in value: problems.append("missing " + key)
    elif not isinstance(value[key], kind) or (kind is bool and isinstance(value[key], bool) is False):
        problems.append("bad " + key + "=" + repr(value[key]))
print("OK" if not problems else "; ".join(problems) + "; observed=" + json.dumps(value, sort_keys=True))
' "$1"
}
first_delivery() {
  python3 -I -c '
import json, sys
try:
    ids = json.load(open(sys.argv[1])).get("delivery_ids") or []
    print(ids[0] if ids else "")
except Exception:
    print("")
' "$1"
}

if [ ! -x "$bin" ]; then bad "binary present" "no executable at $bin"; exit 1; fi
pass "binary present"

tmp="$(mktemp -d /tmp/wire-hold-proof.XXXXXX)"
data="$tmp/data"
mkdir -p "$data"
log="$tmp/test.log"
token="wire-proof-$$-$RANDOM"
port="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
url="http://127.0.0.1:$port"
hold_base="http://127.0.0.1:8649/api/maintenance/hold"
full_hold_url="$hold_base?timeout_s=20&pause_s=30"
expiry_hold_url="$hold_base?timeout_s=20&pause_s=2"
# The app reads the Launcher token from $HOME/Library/KHUpdater/hold-token, and a missing file is now
# a refusal (row 20261123-77). The proof owns a temp HOME with a token so it tests the real contract
# without depending on whether this Mac's Launcher has written one.
hold_token="wire-hold-proof-$$"
own_dir="$tmp/home/Library/Application Support/com.terranash.wirestudiokh"
legacy_dir="$tmp/home/Library/KHUpdater"
own_token="$own_dir/hold-token"
legacy_token="$legacy_dir/hold-token"
mkdir -p "$own_dir" "$legacy_dir"
printf '%s' "$hold_token" > "$own_token"
chmod 600 "$own_token"
pid=""

# The hold port is fixed, so refuse to run beside another copy: the calls would
# reach that process and every assert below would test the wrong app.
if lsof -nP -iTCP:8649 -sTCP:LISTEN >/dev/null 2>&1; then
  echo "FAIL hold port 8649 is already in use by another process; stop it first" >&2
  exit 1
fi

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
# symlink below is in place, so one data dir survives a kill -9 and its restart.
launch() {
  pid=""
  rm -f "$tmp/go" "$tmp/pidfile" "$log"
  TMPDIR="$tmp" bash -c '
    echo $$ > "$1"
    while [ ! -f "$2" ]; do sleep 0.05; done
    shift 2
    exec "$@"
  ' _ "$tmp/pidfile" "$tmp/go" env TMPDIR="$tmp" HOME="$tmp/home" WIRE_STUDIO_KH_DATA_DIR="$data" \
      WIRE_OPERATOR_TOKEN="$token" KH_TEST_PORT="$port" "$bin" --test-mode --KH_TEST_PORT "$port" \
      >"$log" 2>&1 &
  local tries
  for tries in $(seq 1 100); do [ -s "$tmp/pidfile" ] && break; sleep 0.05; done
  pid="$(cat "$tmp/pidfile" 2>/dev/null || true)"
  [ -n "$pid" ] || { bad "test-mode launch wrote a pid" "none"; return 1; }
  ln -sfn "$data" "$tmp/wire-studio-kh-test-$pid"
  : > "$tmp/go"
  for tries in $(seq 1 100); do
    curl -fsS -m 1 "$url/api/test/state" >/dev/null 2>&1 && return 0
    kill -0 "$pid" 2>/dev/null || return 1
    sleep 0.1
  done
  return 1
}
now_state() { curl -fsS -m 5 "$url/api/test/state" > "$tmp/state" 2>/dev/null || true; }
schedule() {
  # Default 5 s so the rows are still queued while the hold assertions run; a
  # due-now row is claimed within a millisecond and would already be terminal.
  local delay="${3:-5}"
  curl -sS -m 5 -o "$1" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
    -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
    --data "{\"text\":\"$2\",\"targets\":[\"telegram:TESTCHAT\"],\"delay_s\":$delay}" || true
}
hold_post() {
  if [ -n "$hold_token" ]; then
    curl -sS -m 35 -o "$2" -w '%{http_code}' -X POST "$1" -H "x-hold-token: $hold_token" || true
  else
    curl -sS -m 35 -o "$2" -w '%{http_code}' -X POST "$1" || true
  fi
}
hold_delete() {
  if [ -n "$hold_token" ]; then
    curl -sS -m 5 -o "$2" -w '%{http_code}' -X DELETE "$1" -H "x-hold-token: $hold_token" || true
  else
    curl -sS -m 5 -o "$2" -w '%{http_code}' -X DELETE "$1" || true
  fi
}

run_full() {
  now_state
  local code code2 post_id cancel_id keep_id cancel_state keep_state paused resumed
  code="$(curl -sS -m 5 -o "$tmp/anon" -w '%{http_code}' -X POST "$url/api/test/trigger/schedule" \
    -H 'Content-Type: application/json' \
    --data '{"text":"no token","targets":["telegram:TESTCHAT"],"delay_s":0}' || true)"
  if [ "$code" = "403" ] && [ "$(json_field "$tmp/anon" error)" = "guard: operator credential required" ]; then
    pass "schedule without operator token refused"
  else
    bad "schedule without operator token refused" "HTTP $code error=$(json_field "$tmp/anon" error)"
  fi

  code="$(schedule "$tmp/schedule1" "hold proof cancel")"
  cancel_id="$(first_delivery "$tmp/schedule1")"
  code2="$(schedule "$tmp/schedule2" "hold proof keep")"
  keep_id="$(first_delivery "$tmp/schedule2")"
  post_id="$(json_field "$tmp/schedule1" post_id)"
  if [ "$code" = "200" ] && [ "$code2" = "200" ] && [ -n "$post_id" ] && [ -n "$cancel_id" ] && [ -n "$keep_id" ]; then
    pass "live queue scheduled: post $post_id rows $cancel_id, $keep_id"
  else
    bad "live queue scheduled with delay 0 to telegram:TESTCHAT" "HTTP $code/$code2 post=$post_id cancel=$cancel_id keep=$keep_id"
    return 1
  fi

  # The hold must refuse an unauthenticated caller with 401 and pause nothing (row 20261123-77):
  # Sparkle checks the feed only after the hold, so a green light here would skip it.
  # Wire's own token file, then the one-time move from the estate path, then neither (0.5.2).
  code="$(curl -sS -m 5 -o "$tmp/own" -w '%{http_code}' -X POST "$full_hold_url" \
    -H "x-hold-token: $hold_token" || true)"
  if [ "$code" = "200" ]; then
    pass "hold reads Wire's own token file"
  else
    bad "hold reads Wire's own token file" "HTTP $code body=$(cat "$tmp/own" 2>/dev/null)"
  fi
  rm -f "$own_token"; printf '%s' "$hold_token" > "$legacy_token"; chmod 600 "$legacy_token"
  code="$(curl -sS -m 5 -o "$tmp/moved" -w '%{http_code}' -X POST "$full_hold_url" \
    -H "x-hold-token: $hold_token" || true)"
  if [ "$code" = "200" ] && [ -s "$own_token" ]; then
    pass "the estate token is copied to Wire's own file once, and the copy answers"
  else
    bad "the estate token is copied to Wire's own file once, and the copy answers" \
      "HTTP $code own=$( [ -s "$own_token" ] && echo present || echo absent)"
  fi
  rm -f "$own_token" "$legacy_token"
  code="$(curl -sS -m 5 -o "$tmp/no-token" -w '%{http_code}' -X POST "$full_hold_url" || true)"
  if [ "$code" = "401" ]; then
    pass "with neither file the hold refuses 401 loudly"
  else
    bad "with neither file the hold refuses 401 loudly" "HTTP $code body=$(cat "$tmp/no-token" 2>/dev/null)"
  fi
  printf '%s' "$hold_token" > "$own_token"; chmod 600 "$own_token"
  code="$(curl -sS -m 5 -o "$tmp/no-token" -w '%{http_code}' -X POST "$full_hold_url" || true)"
  if [ "$code" = "401" ] && [ "$(json_field "$tmp/no-token" error)" = "hold refused: token" ]; then
    pass "hold without a token refused 401"
  else
    bad "hold without a token refused 401" "HTTP $code body=$(cat "$tmp/no-token" 2>/dev/null)"
  fi
  code="$(curl -sS -m 5 -o "$tmp/bad-token" -w '%{http_code}' -X POST "$full_hold_url" \
    -H 'x-hold-token: not-the-token' || true)"
  if [ "$code" = "401" ]; then
    pass "hold with a wrong token refused 401"
  else
    bad "hold with a wrong token refused 401" "HTTP $code body=$(cat "$tmp/bad-token" 2>/dev/null)"
  fi
  if [ -n "$hold_token" ]; then
    code="$(curl -sS -m 5 -o "$tmp/bearer" -w '%{http_code}' -X DELETE "$full_hold_url" \
      -H "Authorization: Bearer $hold_token" || true)"
    if [ "$code" = "200" ]; then
      pass "hold accepts Authorization: Bearer with the same token (the shim's header)"
    else
      bad "hold accepts Authorization: Bearer with the same token (the shim's header)" \
        "HTTP $code body=$(cat "$tmp/bearer" 2>/dev/null)"
    fi
    code="$(curl -sS -m 5 -o "$tmp/origin" -w '%{http_code}' -X POST "$full_hold_url" \
      -H 'Origin: https://evil.example' -H "x-hold-token: $hold_token" || true)"
  else
    code="$(curl -sS -m 5 -o "$tmp/origin" -w '%{http_code}' -X POST "$full_hold_url" \
      -H 'Origin: https://evil.example' || true)"
  fi
  if [ "$code" = "403" ] && [ "$(json_field "$tmp/origin" error)" = "hold refused: origin" ]; then
    pass "foreign Origin refused"
  else
    bad "foreign Origin refused" "HTTP $code body=$(cat "$tmp/origin" 2>/dev/null)"
  fi

  code="$(hold_post "$full_hold_url" "$tmp/hold")"
  local contract
  contract="$(contract_keys "$tmp/hold")"
  if [ "$code" = "200" ] && [ "$contract" = "OK" ]; then
    pass "hold reply has all four contract keys with the right types"
  else
    bad "hold reply has all four contract keys with the right types" "HTTP $code $contract"
  fi
  local safe paused_s
  safe="$(json_field "$tmp/hold" safe)"
  case "$safe" in
    true|false) pass "hold reply safe is boolean ($safe)" ;;
    *) bad "hold reply safe is boolean" "observed $safe" ;;
  esac
  paused_s="$(json_field "$tmp/hold" dispatch_paused_s)"
  case "$paused_s" in
    30|30.0) pass "hold reply dispatch_paused_s is 30" ;;
    *) bad "hold reply dispatch_paused_s is 30" "observed $paused_s" ;;
  esac
  now_state
  paused="$(json_field "$tmp/state" paused)"
  if [ "$paused" = "true" ]; then pass "hold closed dispatch (paused=true)"; else bad "hold closed dispatch (paused=true)" "observed paused=$paused"; fi

  code="$(curl -sS -m 5 -o "$tmp/cancel" -w '%{http_code}' -X POST "$url/api/test/trigger/cancel" \
    -H "x-wire-operator: $token" -H 'Content-Type: application/json' \
    --data "{\"id\":\"$cancel_id\"}" || true)"
  now_state
  cancel_state="$(row_state "$tmp/state" "$cancel_id")"
  if [ "$code" = "200" ] && [ "$cancel_state" = "cancelled" ]; then
    pass "cancelled row $cancel_id is cancelled"
  else
    bad "cancelled row $cancel_id is cancelled" "HTTP $code state=$cancel_state body=$(cat "$tmp/cancel" 2>/dev/null)"
  fi
  keep_state="$(row_state "$tmp/state" "$keep_id")"
  case "$keep_state" in
    pending|queued) pass "other row $keep_id is still pending ($keep_state)" ;;
    *) bad "other row $keep_id is still pending" "observed state=$keep_state" ;;
  esac

  now_state
  paused="$(json_field "$tmp/state" paused)"
  if [ "$paused" != "true" ]; then bad "pause active before kill -9" "observed paused=$paused"; fi
  kill -9 "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  pass "killed -9 mid-pause"

  if launch; then pass "restarted with the same data dir"; else bad "restarted with the same data dir" "server did not come up; log at $log"; return 1; fi
  now_state
  keep_state="$(row_state "$tmp/state" "$keep_id")"
  # The contract asks for a followable id, not a particular state. With no bot
  # token the restarted dispatcher legitimately parks the row at "attention".
  case "$keep_state" in
    pending|queued|attention) pass "row $keep_id survived kill -9 with its id ($keep_state)" ;;
    *) bad "row $keep_id did not survive kill -9" "observed state=$keep_state" ;;
  esac

  resumed=""
  local tries
  for tries in $(seq 1 150); do
    now_state
    paused="$(json_field "$tmp/state" paused)"
    keep_state="$(row_state "$tmp/state" "$keep_id")"
    if [ "$paused" = "false" ] && [ -n "$keep_state" ] && [ "$keep_state" != "pending" ] && [ "$keep_state" != "queued" ]; then
      resumed="$keep_state"
      break
    fi
    sleep 0.1
  done
  if [ -n "$resumed" ]; then pass "dispatch resumed after restart without DELETE (row $resumed)"; else bad "dispatch resumed after restart without DELETE" "paused=$paused row=$keep_state after 15s"; fi

  code="$(hold_post "$full_hold_url" "$tmp/hold2")"
  now_state
  paused="$(json_field "$tmp/state" paused)"
  if [ "$code" = "200" ] && [ "$paused" = "true" ]; then pass "hold before DELETE paused dispatch"; else bad "hold before DELETE paused dispatch" "HTTP $code paused=$paused"; fi
  code="$(hold_delete "$hold_base" "$tmp/resume")"
  local resumed_field
  resumed_field="$(json_field "$tmp/resume" resumed)"
  now_state
  paused="$(json_field "$tmp/state" paused)"
  if [ "$code" = "200" ] && [ "$resumed_field" = "true" ] && [ "$paused" = "false" ]; then
    pass "DELETE resumes dispatch"
  else
    bad "DELETE resumes dispatch" "HTTP $code resumed=$resumed_field paused=$paused"
  fi
}

run_expiry() {
  local code row_id paused state tried
  code="$(schedule "$tmp/expiry-schedule" "expiry proof" 0)"
  row_id="$(first_delivery "$tmp/expiry-schedule")"
  if [ "$code" = "200" ] && [ -n "$row_id" ]; then
    pass "expiry proof queued live row $row_id with delay 0 to telegram:TESTCHAT"
  else
    bad "expiry proof queued live row" "HTTP $code id=$row_id body=$(cat "$tmp/expiry-schedule" 2>/dev/null)"
    return 1
  fi
  code="$(hold_post "$expiry_hold_url" "$tmp/expiry-hold")"
  if [ "$code" = "200" ]; then pass "hold with pause_s=2 accepted"; else bad "hold with pause_s=2 accepted" "HTTP $code body=$(cat "$tmp/expiry-hold" 2>/dev/null)"; fi
  now_state
  paused="$(json_field "$tmp/state" paused)"
  if [ "$paused" = "true" ]; then pass "hold paused dispatch before expiry"; else bad "hold paused dispatch before expiry" "observed paused=$paused"; fi

  sleep 3
  now_state
  paused="$(json_field "$tmp/state" paused)"
  if [ "$paused" = "false" ]; then pass "hold expires by itself after 2s with no DELETE"; else bad "hold expires by itself after 2s with no DELETE" "observed paused=$paused after 3s"; fi
  state="$(row_state "$tmp/state" "$row_id")"
  if [ -n "$state" ]; then pass "queued row $row_id still present after expiry (state $state)"; else bad "queued row $row_id still present after expiry" "row missing"; fi

  resumed=""
  for tried in $(seq 1 50); do
    now_state
    state="$(row_state "$tmp/state" "$row_id")"
    if [ -n "$state" ] && [ "$state" != "pending" ] && [ "$state" != "queued" ]; then resumed="$state"; break; fi
    sleep 0.1
  done
  if [ -n "$resumed" ]; then pass "dispatch resumed after expiry (row $resumed)"; else bad "dispatch resumed after expiry" "row=$state after 5s"; fi
}

if launch; then pass "test-mode server ready on $port"; else bad "test-mode server ready on $port" "log at $log"; exit 1; fi
printed_dir="$(grep -m1 '^TEST_DATA_DIR=' "$log" || true)"
case "$printed_dir" in
  "TEST_DATA_DIR=$tmp"*) pass "TEST_DATA_DIR is the fresh temp data dir" ;;
  *) bad "TEST_DATA_DIR is the fresh temp data dir" "$printed_dir (expected under $tmp)" ;;
esac
[ -f "$data/wire.db" ] && pass "queue database in temp data dir" || bad "queue database in temp data dir" "wire.db missing in $data"

if [ "$expiry_only" -eq 0 ]; then run_full; fi
run_expiry

printf '%s\n' "RESULT fails=$fails"
[ "$fails" -eq 0 ] && exit 0
exit 1
