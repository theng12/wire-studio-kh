#!/bin/bash
# Public-release secret scan of the exact DMG being published: mount read-only,
# grep every text-bearing file inside the app bundle, then detach.
# Usage: scan-dmg.sh <path.dmg>
set -u
DMG="${1:?usage: scan-dmg.sh <path.dmg>}"
PATTERN='sk-ant-|sk-[A-Za-z0-9]{20,}|ghp_|gho_|github_pat_|tskey-|AKIA|xoxb-|bot[0-9]{8,}:[A-Za-z0-9_-]{30,}|BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY|client_secret[[:space:]]*[:=]'
mnt="$(mktemp -d /tmp/wire-scan.XXXXXX)"
hits=0
hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$mnt" >/dev/null || { echo "FAIL cannot mount $DMG"; exit 1; }
trap 'hdiutil detach "$mnt" >/dev/null 2>&1; rmdir "$mnt" 2>/dev/null' EXIT
app="$(find "$mnt" -maxdepth 2 -name '*.app' -print -quit)"
[ -n "$app" ] || { echo "FAIL no .app inside $DMG"; exit 1; }
while IFS= read -r f; do
  if strings "$f" 2>/dev/null | grep -Eq -- "$PATTERN"; then
    echo "  secret-like string in ${f#"$mnt"/}" >&2
    hits=$((hits+1))
  fi
done < <(find "$app" -type f \( -perm -111 -o -name '*.plist' -o -name '*.json' -o -name '*.js' -o -name '*.css' -o -name '*.html' -o -name '*.sh' \))
if [ "$hits" -gt 0 ]; then
  echo "FAIL $hits file(s) with secret-like strings" >&2
  exit 1
fi
echo "PASS no secret-like strings in $(basename "$DMG")"
