#!/bin/bash
# Writes docs/appcast.xml for the DMG it is given. Pages serves /docs, and Sparkle reads
# https://theng12.github.io/wire-studio-kh/appcast.xml.
#
# The EdDSA signature covers THESE exact DMG bytes: a re-cut DMG must regenerate its item, because
# a stale signature fails silently — the app simply never updates.
#
#   bash scripts/appcast.sh "dist/Wire Studio KH_0.5.1_aarch64.dmg"
set -euo pipefail
cd "$(dirname "$0")/.."

fail() { printf 'Stopped: %s\n' "$1" >&2; exit 1; }

dmg="${1:?usage: appcast.sh <dist-dmg>}"
[ -f "$dmg" ] || fail "no DMG at $dmg"
version="$(cat VERSION)"
name="Wire Studio KH"
appcast="docs/appcast.xml"
sign="src-tauri/vendor/bin/sign_update"
private_key="${WIRE_SPARKLE_PRIVATE:-$HOME/.kh-sparkle/com.terranash.wirestudiokh/private.key}"

[ -x "$sign" ] || fail "the vendored sign_update is missing at $sign"
[ -f "$private_key" ] || fail "no Sparkle private key at $private_key — set WIRE_SPARKLE_PRIVATE"
chmod 600 "$private_key" 2>/dev/null || true

asset="$(basename "$dmg" | tr ' ' '.')"
url="https://github.com/theng12/wire-studio-kh/releases/download/wire-${version}/${asset}"
length="$(stat -f %z "$dmg")"

# -p prints just the EdDSA signature; the appcast carries it as sparkle:edSignature.
signature="$("$sign" --ed-key-file "$private_key" -p "$dmg")"
[ -n "$signature" ] || fail "sign_update returned no signature"

mkdir -p docs
[ -f "$appcast" ] || cat > "$appcast" <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>$name</title>
    <link>https://theng12.github.io/wire-studio-kh/appcast.xml</link>
    <description>$name releases</description>
    <language>en</language>
  </channel>
</rss>
XML

python3 - "$appcast" "$version" "$url" "$signature" "$length" <<'PY'
import sys, datetime, xml.etree.ElementTree as ET

path, version, url, signature, length = sys.argv[1:6]
tree = ET.parse(path)
root = tree.getroot()
channel = root.find("channel")
SPARKLE = "http://www.andymatuschak.org/xml-namespaces/sparkle"
ET.register_namespace("sparkle", SPARKLE)

for item in channel.findall("item"):
    if (item.findtext("{%s}shortVersionString" % SPARKLE) or item.findtext("title") or "").strip() == version:
        sys.exit(f"refused: {path} already carries an item for {version}; one version = one item")

item = ET.Element("item")
ET.SubElement(item, "title").text = version
ET.SubElement(item, "pubDate").text = datetime.datetime.now(datetime.timezone.utc).strftime(
    "%a, %d %b %Y %H:%M:%S +0000")
ET.SubElement(item, "{%s}version" % SPARKLE).text = version
ET.SubElement(item, "{%s}shortVersionString" % SPARKLE).text = version
ET.SubElement(item, "{%s}minimumSystemVersion" % SPARKLE).text = "10.13"
enclosure = ET.SubElement(item, "enclosure")
enclosure.set("url", url)
enclosure.set("length", length)
enclosure.set("type", "application/octet-stream")
enclosure.set("{%s}version" % SPARKLE, version)
enclosure.set("{%s}shortVersionString" % SPARKLE, version)
enclosure.set("{%s}edSignature" % SPARKLE, signature)

channel.insert(list(channel).index(channel.find("item")) if channel.find("item") is not None else len(channel), item)
tree.write(path, encoding="utf-8", xml_declaration=True)
print(f"wrote {path}: item {version}, length {length}, signature {signature[:12]}…")
PY
