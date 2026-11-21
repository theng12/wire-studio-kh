#!/bin/bash
# Public-artifact scan: what a download must never reveal, and the one exact
# exemption that keeps the rule usable.
#
# Fails on: absolute user paths (/Users/...), tailnet hostnames/URLs (*.ts.net,
# terranash-*), personal email addresses, and key/token material.
# Allows: the string "terranash" ONLY as part of "com.terranash." — the public
# bundle id, which is on the feed and in every install BY DESIGN. This is an
# exact-prefix exemption with its reason, not a blanket ignore of the word.
#
# Usage: scan-public.sh <file-or-dmg> [more files...]
#        scan-public.sh --self-test
set -u

scan_python='
import re, sys
FORBIDDEN = [
    (re.compile(rb"/Users/[^/\x00\s\x22\x27]+"), "absolute user path"),
    (re.compile(rb"[A-Za-z0-9-]+\.ts\.net"), "tailnet hostname"),
    (re.compile(rb"\bterranash-[A-Za-z0-9-]+"), "host name"),
    (re.compile(rb"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"), "email address"),
    (re.compile(rb"bot[0-9]{8,}:[A-Za-z0-9_-]{30,}"), "telegram bot token"),
    (re.compile(rb"ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}"), "github token"),
    (re.compile(rb"tskey-[A-Za-z0-9-]{20,}"), "tailscale key"),
    (re.compile(rb"AKIA[0-9A-Z]{16}"), "aws key"),
    (re.compile(rb"BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY"), "private key"),
]
hits = []
for path in sys.argv[1:]:
    data = open(path, "rb").read()
    for rx, label in FORBIDDEN:
        for m in rx.finditer(data):
            hits.append((path, label, m.start(), m.group()[:60]))
    # "terranash" is allowed only when the four bytes before it are "com."
    start = 0
    while True:
        i = data.find(b"terranash", start)
        if i < 0:
            break
        if data[max(0, i - 4):i] != b"com.":
            hits.append((path, "terranash outside com.terranash.", i,
                         data[max(0, i - 12):i + 24]))
        start = i + 1
for path, label, off, ctx in hits:
    print(f"FAIL {path} @{off}: {label}: {ctx!r}")
print(f"RESULT hits={len(hits)}")
sys.exit(0 if not hits else 1)
'

scan_one() {
  local target="$1"
  case "$target" in
    *.dmg)
      local mnt; mnt="$(mktemp -d /tmp/wire-scan.XXXXXX)"
      hdiutil attach "$target" -nobrowse -readonly -mountpoint "$mnt" >/dev/null 2>&1 || { echo "FAIL cannot mount $target"; return 1; }
      local rc=0
      while IFS= read -r f; do
        python3 -I -c "$scan_python" "$f" || rc=1
      done < <(find "$mnt" -type f \( -perm -111 -o -name '*.plist' -o -name '*.json' -o -name '*.js' -o -name '*.css' -o -name '*.html' -o -name '*.sh' -o -name '*.txt' \))
      hdiutil detach "$mnt" >/dev/null 2>&1; rmdir "$mnt" 2>/dev/null
      return "$rc"
      ;;
    *) python3 -I -c "$scan_python" "$target" ;;
  esac
}

self_test() {
  local dir; dir="$(mktemp -d /tmp/wire-scan-test.XXXXXX)"
  printf '/Users/x/y' > "$dir/bad_path"
  printf 'terranash-1301-m1-16-1000' > "$dir/bad_host"
  printf 'someone@example.com' > "$dir/bad_email"
  printf 'com.terranash.wirestudiokh' > "$dir/good_bundle"
  local rc_bad=0 rc_good=1
  python3 -I -c "$scan_python" "$dir/bad_path" >/dev/null 2>&1 || rc_bad=$?
  python3 -I -c "$scan_python" "$dir/good_bundle" >/dev/null 2>&1; rc_good=$?
  local ok=1
  [ "$rc_bad" -eq 1 ] && echo "PASS synthetic /Users/x/y is caught" || { echo "FAIL synthetic /Users/x/y was not caught"; ok=0; }
  python3 -I -c "$scan_python" "$dir/bad_host" >/dev/null 2>&1 && { echo "FAIL synthetic hostname was not caught"; ok=0; } || echo "PASS synthetic hostname is caught"
  python3 -I -c "$scan_python" "$dir/bad_email" >/dev/null 2>&1 && { echo "FAIL synthetic email was not caught"; ok=0; } || echo "PASS synthetic email is caught"
  [ "$rc_good" -eq 0 ] && echo "PASS com.terranash.wirestudiokh is allowed" || { echo "FAIL bundle id was refused"; ok=0; }
  rm -rf "$dir"
  [ "$ok" -eq 1 ]
}

[ "${1:-}" = "--self-test" ] && { self_test; exit $?; }
[ $# -gt 0 ] || { echo "usage: scan-public.sh <file-or-dmg> [...] | --self-test"; exit 2; }
rc=0
for t in "$@"; do scan_one "$t" || rc=1; done
[ "$rc" -eq 0 ] && echo "PASS public artifact scan" || echo "FAIL public artifact scan"
exit "$rc"
