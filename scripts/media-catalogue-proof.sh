#!/usr/bin/env bash
# Wire 0.5.0 Business proof: read-only Media catalogue, product on the composed post, live price.
#
#   bash scripts/media-catalogue-proof.sh
#
# Proves, with no window and nothing written to Media's real store:
#   1 the catalogue is read from Media's SQLite file and the file is byte-identical afterwards
#   2 a product can be attached to a queued post, and the price shown at compose time is recorded
#   3 a price changed 2 s before send appears in the text that will be sent, with its "as of" time
#   4 the row records both the shown and the sent price (no cache past a change)
#   5 a store that is missing or unreadable says so, and never reads as an empty catalogue
set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${WIRE_BIN:-$root/src-tauri/target/debug/wire-studio-kh}"
[ -x "$bin" ] || bin="$root/src-tauri/target/release/wire-studio-kh"

fails=0
pass() { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s (observed %s)\n' "$1" "$2"; fails=$((fails + 1)); }
field() {
  python3 -I -c '
import json, sys
try:
    value = json.load(open(sys.argv[1]))
    for key in sys.argv[2].split("."):
        value = value[int(key)] if isinstance(value, list) else value[key]
    print(json.dumps(value, separators=(",", ":")) if isinstance(value, (list, dict)) else
          ("" if value is None else str(value).lower() if isinstance(value, bool) else str(value)))
except Exception:
    print("")
' "$1" "$2"
}

[ -x "$bin" ] || { bad "binary present" "no executable at $bin"; exit 1; }
pass "binary present"

tmp="$(mktemp -d /tmp/wire-media-proof.XXXXXX)"
pid=""
cleanup() {
  rc=$?
  [ -n "$pid" ] && kill -9 "$pid" 2>/dev/null
  [ -n "${wal_pid:-}" ] && kill -9 "$wal_pid" 2>/dev/null
  wait 2>/dev/null
  rm -rf "$tmp"
  exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ---------- a stub Media store with the real schema ----------
store="$tmp/media/database.sqlite"
mkdir -p "$tmp/media"
python3 -I - "$store" <<'PY'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
db.executescript("""
CREATE TABLE products (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, sku TEXT NOT NULL, name TEXT,
  brand_id TEXT, barcode TEXT, secondary_code TEXT, category_id TEXT, subcategory TEXT,
  color_finish TEXT, variant TEXT, unit TEXT, status TEXT NOT NULL DEFAULT 'active',
  tags TEXT NOT NULL DEFAULT '[]', description TEXT, price_retail REAL, price_wholesale REAL,
  process_status TEXT NOT NULL DEFAULT 'unprocessed', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE companies (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
INSERT INTO companies VALUES ('c1','Wire Proof Co',0,0);
INSERT INTO products (id, company_id, sku, name, status, price_retail, created_at, updated_at)
  VALUES ('p1','c1','SKU-1','Krama Scarf','active',12.5,0,0),
         ('p2','c1','SKU-2','Lotus Tea','active',8.0,0,0),
         ('p3','c1','SKU-3','Retired Item','archived',3.0,0,0);
""")
db.commit()
db.close()
PY
store_state() { stat -f '%m %z' "$store"; shasum -a 256 "$store" | awk '{print $1}'; }
# A live WAL: Media running with an open connection leaves -wal/-shm beside the file, which is the
# case a read-only open can fail on. Hold one open for the whole proof.
cat > "$tmp/wal-holder.py" <<'WALPY'
import sqlite3, sys, time
db = sqlite3.connect(sys.argv[1])
db.execute("PRAGMA journal_mode=WAL")
db.execute("INSERT INTO products (id, company_id, sku, name, status, price_retail, created_at, updated_at) "
           "VALUES ('p4','c1','SKU-4','Unpriced Item','active',NULL,0,0)")
db.commit()
print("wal-held", flush=True)
time.sleep(120)
WALPY
python3 -I "$tmp/wal-holder.py" "$store" > "$tmp/wal.log" 2>&1 &
wal_pid=$!
for _ in $(seq 1 50); do grep -q 'wal-held' "$tmp/wal.log" 2>/dev/null && break; sleep 0.1; done
before="$(store_state)"
if [ -e "$tmp/media/database.sqlite-wal" ]; then
  pass "stub store is live: -wal present while a writer holds it open"
else
  bad "stub store is live: -wal present while a writer holds it open" "no -wal beside the store"
fi
pass "stub Media store written ($(wc -c < "$store" | tr -d ' ') bytes)"

# ---------- Wire, hidden, own data dir ----------
port="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
token="wire-media-proof-$$-$RANDOM"
url="http://127.0.0.1:$port"
data="$tmp/wire-data"
mkdir -p "$data"
TMPDIR="$tmp" WIRE_STUDIO_KH_DATA_DIR="$data" WIRE_OPERATOR_TOKEN="$token" \
  "$bin" --test-mode --KH_TEST_PORT "$port" > "$tmp/wire.log" 2>&1 &
pid=$!
for _ in $(seq 1 120); do
  grep -q '^TEST_DATA_DIR=' "$tmp/wire.log" 2>/dev/null && break
  kill -0 "$pid" 2>/dev/null || break
  sleep 0.1
done
grep -q '^TEST_DATA_DIR=' "$tmp/wire.log" 2>/dev/null || { bad "Wire test-mode started" "$(tail -3 "$tmp/wire.log" | tr '\n' ' ')"; exit 1; }
pass "Wire test-mode started"

trigger() { # trigger <route> <json> <outfile> -> http code
  curl -sS -m 15 -o "$3" -w '%{http_code}' -X POST "$url/api/test/trigger/$1" \
    -H "x-wire-operator: $token" -H 'Content-Type: application/json' --data "$2" || true
}

# 1 — read the catalogue; Media's file must not change.
code="$(trigger media-path "{\"path\":\"$store\"}" "$tmp/point")"
[ "$(field "$tmp/point" found)" = "true" ] && pass "the store path is accepted and found" \
  || bad "the store path is accepted and found" "HTTP $code body=$(cat "$tmp/point" 2>/dev/null)"
code="$(trigger media-products '{"limit":10}' "$tmp/products")"
count="$(python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); print(len(d.get("products") or []))' "$tmp/products" 2>/dev/null || echo 0)"
name="$(field "$tmp/products" products.0.name)"
price="$(field "$tmp/products" products.0.price_retail)"
[ "$code" = "200" ] && [ "$count" = "3" ] && [ "$name" = "Krama Scarf" ] && [ "$price" = "12.5" ] \
  && pass "active products are listed, archived ones excluded, price read live" \
  || bad "active products are listed, archived ones excluded, price read live" "HTTP $code count=$count name=$name price=$price"
[ -n "$(field "$tmp/products" as_of)" ] && pass "the catalogue carries an as-of time" \
  || bad "the catalogue carries an as-of time" "as_of empty"
after_read="$(store_state)"
[ "$before" = "$after_read" ] && pass "Media's store is byte-identical after Wire reads it" \
  || bad "Media's store is byte-identical after Wire reads it" "before=[$before] after=[$after_read]"
# The assertion covers database.sqlite only (mtime + sha256). Sidecars are reported, not claimed.
printf 'NOTE sidecars beside the store after Wire reads: %s\n' "$(ls "$tmp/media" | tr '\n' ' ')"

# 2 — attach the product to a queued post; the shown price is recorded.
code="$(trigger schedule '{"text":"New scarf colours are in","targets":["telegram:TESTCHAT"],"product_id":"p1"}' "$tmp/queued")"
row="$(field "$tmp/queued" delivery_ids.0)"
[ -n "$row" ] && pass "a post with a product is queued" || bad "a post with a product is queued" "HTTP $code body=$(cat "$tmp/queued" 2>/dev/null)"
curl -sS -m 5 -o "$tmp/state" "$url/api/test/state" >/dev/null 2>&1
shown="$(python3 -I -c '
import json,sys
rows=json.load(open(sys.argv[1])).get("deliveries") or []
row=[r for r in rows if r.get("id")==sys.argv[2]]
print(row[0].get("price_shown") if row else "missing")' "$tmp/state" "$row" 2>/dev/null)"
[ "$shown" = "12.5" ] && pass "the row records the price shown when the post was composed" \
  || bad "the row records the price shown when the post was composed" "price_shown=$shown"

# 3/4 — change the price 2 s before send; the text that will be sent carries the new one.
sleep 2
t_change="$(date -u +%s)"
python3 -I -c '
import sqlite3, sys
db = sqlite3.connect(sys.argv[1]); db.execute("UPDATE products SET price_retail=15.75, updated_at=1 WHERE id=?", ("p1",)); db.commit(); db.close()' "$store"
changed="$(store_state)"
[ "$changed" != "$before" ] && pass "the stub catalogue's price changed (by the test, not by Wire)" \
  || bad "the stub catalogue's price changed (by the test, not by Wire)" "hash unchanged"
code="$(trigger preview-send "{\"id\":\"$row\"}" "$tmp/preview")"
text="$(field "$tmp/preview" text)"
case "$text" in
  *15.75*) pass "the text that will be sent carries the new price" ;;
  *) bad "the text that will be sent carries the new price" "HTTP $code text=$text" ;;
esac
case "$text" in
  *"(as of "*) pass "the sent text names when the price was read" ;;
  *) bad "the sent text names when the price was read" "text=$text" ;;
esac
curl -sS -m 5 -o "$tmp/state2" "$url/api/test/state" >/dev/null 2>&1
sent="$(python3 -I -c '
import json,sys
rows=json.load(open(sys.argv[1])).get("deliveries") or []
row=[r for r in rows if r.get("id")==sys.argv[2]]
print(row[0].get("price_sent") if row else "missing")' "$tmp/state2" "$row" 2>/dev/null)"
[ "$sent" = "15.75" ] && pass "the row records the sent price beside the shown one" \
  || bad "the row records the sent price beside the shown one" "price_sent=$sent"
as_of="$(python3 -I -c '
import json,sys
rows=json.load(open(sys.argv[1])).get("deliveries") or []
row=[r for r in rows if r.get("id")==sys.argv[2]]
print(row[0].get("price_as_of") if row else "")' "$tmp/state2" "$row" 2>/dev/null)"
as_of_verdict="$(python3 -I - "$as_of" "$t_change" "$text" <<'ASOF'
import datetime, re, sys
raw, changed, text = sys.argv[1], int(sys.argv[2]), sys.argv[3]
try:
    stamp = datetime.datetime.fromisoformat(raw.replace("Z", "+00:00"))
except Exception:
    print("unparseable:" + raw); raise SystemExit
if stamp.timestamp() < changed:
    print("older-than-the-change:" + raw); raise SystemExit
# The text names the same minute, in Phnom Penh time like every other stamp in the app.
local = stamp.astimezone(datetime.timezone(datetime.timedelta(hours=7))).strftime("%H:%M")
shown = re.search(r"\(as of ([0-9]{2}:[0-9]{2})\)", text)
if not shown:
    print("text-has-no-as-of"); raise SystemExit
print("ok" if shown.group(1) == local else f"text-says-{shown.group(1)}-row-says-{local}")
ASOF
)"
[ "$as_of_verdict" = "ok" ] && pass "the as-of time is the second read and matches the text's clock" \
  || bad "the as-of time is the second read and matches the text's clock" "$as_of_verdict"
after_send="$(store_state)"
[ "$after_send" = "$changed" ] && pass "Media's store is still byte-identical after the send-path read" \
  || bad "Media's store is still byte-identical after the send-path read" "changed=[$changed] after=[$after_send]"

# 4b — a product with no price says so; never $0, never a guessed symbol.
trigger schedule '{"text":"Unpriced post","targets":["telegram:TESTCHAT"],"product_id":"p4"}' "$tmp/queued-null" >/dev/null
row_null="$(field "$tmp/queued-null" delivery_ids.0)"
trigger preview-send "{\"id\":\"$row_null\"}" "$tmp/preview-null" >/dev/null
text_null="$(field "$tmp/preview-null" text)"
case "$text_null" in
  *"price unavailable"*) pass "an unpriced product says the price is unavailable" ;;
  *) bad "an unpriced product says the price is unavailable" "text=$text_null" ;;
esac
case "$text_null" in
  *'$'*|*"0.00"*) bad "no invented currency symbol or zero" "text=$text_null" ;;
  *) pass "no invented currency symbol or zero" ;;
esac

# 5 — honest states for a missing and an unreadable store.
code="$(trigger media-path "{\"path\":\"$tmp/nowhere/database.sqlite\"}" "$tmp/missing")"
[ "$(field "$tmp/missing" found)" = "false" ] && [ -n "$(field "$tmp/missing" reason)" ] \
  && pass "a missing store says so with a reason" \
  || bad "a missing store says so with a reason" "HTTP $code body=$(cat "$tmp/missing" 2>/dev/null)"

mkdir -p "$tmp/locked" && printf 'not a database' > "$tmp/locked/database.sqlite"
code="$(trigger media-path "{\"path\":\"$tmp/locked/database.sqlite\"}" "$tmp/unreadable")"
[ "$(field "$tmp/unreadable" found)" = "false" ] && [ -n "$(field "$tmp/unreadable" reason)" ] \
  && pass "an unreadable store says so with a reason" \
  || bad "an unreadable store says so with a reason" "HTTP $code body=$(cat "$tmp/unreadable" 2>/dev/null)"

# Hidden, always.
if lsof -p "$pid" 2>/dev/null | grep -q 'WindowServer'; then
  bad "no window and no desktop" "WindowServer connection open"
else
  pass "no window and no desktop"
fi

kill -TERM "$pid" 2>/dev/null
exited=""
for _ in $(seq 1 50); do
  if ! kill -0 "$pid" 2>/dev/null; then wait "$pid" 2>/dev/null; exited=$?; break; fi
  sleep 0.1
done
pid=""
[ "$exited" = "0" ] && pass "SIGTERM exits 0" || bad "SIGTERM exits 0" "rc=${exited:-still running}"

if [ "$fails" -eq 0 ]; then
  printf 'MEDIA CATALOGUE PROOF PASS\n'
  exit 0
fi
printf 'MEDIA CATALOGUE PROOF FAILED (%s checks)\n' "$fails"
exit 1
