#!/usr/bin/env bash
# Assemble the static site for GitHub Pages (or any static host) in .dev-cache/pages-site:
#   /             the browser app (PWA) from apps/web/dist
#   /packs/       the content packs (no model: hosts like GitHub Pages refuse files over 100 MB) + a signed catalog
# The model pack is offered as a .lbpack download on the GitHub release; the browser app imports it from a file.
# Needs LBN_SIGNING_KEY (run through build/with-signing-key.sh) to sign the trimmed catalog.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
: "${LBN_SIGNING_KEY:?run through build/with-signing-key.sh}"
OUT=.dev-cache/pages-site; rm -rf "$OUT"; mkdir -p "$OUT/packs"
cp -r apps/web/dist/. "$OUT/"
touch "$OUT/.nojekyll"
python3 - "$OUT" <<'PY'
import json, os, shutil, sys
out = sys.argv[1]
cat = json.load(open("dist/packs/catalog.json"))
keep = [p for p in cat["packs"] if p["kind"] != "model"]
for p in keep:
    d = p["manifest"].split("/")[0]
    shutil.copytree(os.path.join("dist/packs", d), os.path.join(out, "packs", d))
    p.pop("lbpack", None); p.pop("lbpack_bytes", None); p.pop("lbpack_sha256", None)
cat["packs"] = keep
cat["title"] = "Ark sample packs"
open(os.path.join(out, "packs", "catalog.json"), "w").write(json.dumps(cat, indent=1) + "\n")
print(f"site: {len(keep)} packs")
PY
target/release/ark-pack pack sign-catalog "$OUT/packs/catalog.json" "$LBN_SIGNING_KEY"
target/release/ark-pack verify-file "$OUT/packs/catalog.json" >/dev/null && echo "catalog signature verifies"
find "$OUT" -type f -size +95M -print -exec false {} + && du -sh "$OUT"
