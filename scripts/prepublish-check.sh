#!/bin/bash
# Everything that must be true immediately before Wire 0.5.3's row is written, checked in one command
# so it cannot be skipped: the artifact's identity, the appcast item over those exact bytes, and the
# served row's hold pair (with its port) and tagline.
#
#   bash scripts/prepublish-check.sh "dist/Wire Studio KH_0.5.3_aarch64.dmg"
set -uo pipefail
cd "$(dirname "$0")/.."

fail() { printf 'REFUSED: %s\n' "$1" >&2; exit 1; }
dmg="${1:?usage: prepublish-check.sh <artifact>}"
version="$(cat VERSION)"

[ -f "$dmg" ] || fail "no artifact at $dmg"
sha="$(shasum -a 256 "$dmg" | cut -d' ' -f1)"
bytes="$(stat -f %z "$dmg")"
echo "artifact  $dmg"
echo "sha256    $sha"
echo "bytes     $bytes"

# 1. The appcast item must sign THESE bytes and name a published asset URL.
[ -f docs/appcast.xml ] || fail "no docs/appcast.xml"
python3 - "$version" "$bytes" <<'PY' || exit 1
import sys, xml.etree.ElementTree as ET
version, want = sys.argv[1], sys.argv[2]
ns = "{http://www.andymatuschak.org/xml-namespaces/sparkle}"
t = ET.parse("docs/appcast.xml")
item = next((i for i in t.getroot().findall("channel/item")
             if (i.findtext("title") or "").strip() == version), None)
if item is None:
    sys.exit(f"REFUSED: appcast has no item for {version}")
enc = item.find("enclosure")
if enc.get("length") != want:
    sys.exit(f"REFUSED: appcast length {enc.get('length')} != artifact {want}")
url = enc.get("url", "")
if "/releases/download/" not in url:
    sys.exit(f"REFUSED: enclosure is not a published asset URL: {url}")
if not enc.get(ns + "edSignature"):
    sys.exit("REFUSED: enclosure carries no edSignature")
print(f"appcast   item {version}, length {want}, url {url.rsplit('/',1)[-1]}")
PY

# 2. Signature over the artifact itself.
sig="$(python3 -I -c "
import xml.etree.ElementTree as ET
ns='{http://www.andymatuschak.org/xml-namespaces/sparkle}'
t=ET.parse('docs/appcast.xml')
i=next(x for x in t.getroot().findall('channel/item') if (x.findtext('title') or '').strip()=='$version')
print(i.find('enclosure').get(ns+'edSignature'))")"
key="${WIRE_SPARKLE_PRIVATE:-$HOME/.kh-sparkle/com.terranash.wirestudiokh/private.key}"
[ -f "$key" ] || fail "no signing key at $key"
src-tauri/vendor/bin/sign_update --verify --ed-key-file "$key" "$dmg" "$sig" >/dev/null 2>&1 \
  || fail "the signature does not verify over $dmg"
echo "signature verifies over these bytes"

# 3. The served row: hold pair with its port, and a tagline within the 62-character rule.
python3 - <<'PY' || exit 1
import base64, json, sys, urllib.request
try:
    d = json.load(urllib.request.urlopen("https://theng12.github.io/kh-releases/release.json", timeout=20))
    feed = json.loads(base64.b64decode(d["feed"]))
except Exception as exc:
    sys.exit(f"REFUSED: the signed feed could not be read: {exc}")
row = feed.get("apps", {}).get("wire")
if not row:
    sys.exit("REFUSED: the feed has no wire row")
hold = row.get("holdURL") or ""
since = row.get("holdSince") or ""
tag = row.get("tagline") or ""
if not hold.endswith(":8649/api/maintenance/hold"):
    sys.exit(f"REFUSED: wire's holdURL is {hold!r}, expected the 8649 hold route")
if not since:
    sys.exit("REFUSED: wire's row carries no holdSince")
if not tag:
    sys.exit("REFUSED: wire's row carries no tagline")
if len(tag) > 62:
    sys.exit(f"REFUSED: tagline is {len(tag)} characters, over the 62 rule")
print(f"row       hold {hold} since {since}")
print(f"tagline   {tag!r} ({len(tag)} chars)")
print(f"serving   {row.get('version')} sha {(row.get('sha256') or '')[:16]}")
PY
echo "PASS prepublish check for $version"

# 4. §5d: the release is not done without the screenshots, so they are a finding, not a note.
shot_dir=docs/screenshots
count=0
if [ -d "$shot_dir" ]; then
  for png in "$shot_dir"/*.png; do
    [ -e "$png" ] || continue
    size=$(stat -f %z "$png")
    [ "$size" -le 409600 ] || fail "$(basename "$png") is $size bytes, over the 400 KB rule"
    grep -q "$(basename "$png")" README.md || fail "$(basename "$png") is not referenced from README.md"
    count=$((count + 1))
  done
fi
[ "$count" -ge 3 ] || fail "docs/screenshots holds $count image(s); three is the floor"
echo "screenshots $count images, each <=400 KB and referenced from README.md"
