#!/usr/bin/env bash
# Test fixtures for the browser suite: dist/test-packs = the sample packs (hard links) plus
#   docs-v2/          a signed update of the docs pack (version 2026.11.18, changed text)
#   docs-v2-tampered/ the same update with one file changed after signing (must be rejected)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
CLI="$ROOT/target/release/lastbook"; KEY="${LBN_SIGNING_KEY:-$HOME/.config/lastbook-next/keys/sample-publisher.key}"
OUT=dist/test-packs; rm -rf "$OUT"; mkdir -p "$OUT"
cp -al dist/packs/. "$OUT/"
SRC=$(mktemp -d); cp -r packs/sample/docs/. "$SRC/"
sed -i 's/"version": "2026.11.17"/"version": "2026.11.18"/' "$SRC/pack.src.json"
sed -i 's/Store at least one gallon of water per person per day/Store at least one gallon of water per person per day (updated edition)/' "$SRC/res/water-guide.html"
"$CLI" pack build "$SRC" "$OUT/docs-v2" --sign "$KEY" >/dev/null
cp -r "$OUT/docs-v2" "$OUT/docs-v2-tampered"
sed -i 's/updated edition/UPDATED edition/' "$OUT/docs-v2-tampered/res/water-guide.html"
rm -rf "$SRC"; echo "test packs in $OUT"
