#!/bin/bash
# §5d preflight, as a gate step: three images at least, each 400 KB or less, each referenced from
# README.md, and each read by eye at capture time with the result recorded in the receipt.
set -u
cd "$(dirname "$0")/.."

dir=docs/screenshots
count=0
if [ -d "$dir" ]; then
  for png in "$dir"/*.png; do
    [ -e "$png" ] || continue
    name="$(basename "$png")"
    size="$(stat -f %z "$png")"
    if [ "$size" -gt 409600 ]; then
      echo "FAIL $name is $size bytes, over the 400 KB rule"
      exit 1
    fi
    if ! grep -q "$name" README.md; then
      echo "FAIL $name is not referenced from README.md"
      exit 1
    fi
    count=$((count + 1))
  done
fi
if [ "$count" -lt 3 ]; then
  echo "FAIL docs/screenshots holds $count image(s); three is the floor"
  exit 1
fi
echo "PASS $count screenshots, each <=400 KB and referenced from README.md"
