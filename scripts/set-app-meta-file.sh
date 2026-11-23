#!/bin/bash
# Point app-meta.json's localFile at the artifact this version will build.
#
# Run it as part of the version bump, before the commit and before the build: the feed generator
# reads app-meta.json at the tag, so the file must name the tag's artifact when the tag is cut.
# build-release.sh refuses a build whose localFile does not match — that refusal is the backstop,
# not the workflow (S001, 2026-11-23).
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(cat VERSION)"
name="Wire Studio KH"
file="${name}_${version}_aarch64.dmg"

python3 - "$version" "$file" <<'PY'
import json, sys
version, file = sys.argv[1:3]
meta = json.load(open("app-meta.json"))
meta["localFile"] = file
with open("app-meta.json", "w") as stream:
    json.dump(meta, stream, indent=2, ensure_ascii=False)
    stream.write("\n")
print(f"app-meta.json localFile = {file} (version {version})")
PY
