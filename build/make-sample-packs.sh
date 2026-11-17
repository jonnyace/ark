#!/usr/bin/env bash
# Build the sample content and model packs into dist/packs/ (folders for HTTP install, .lbpack files for offline
# transfer) plus a catalog.json listing them. Deterministic: the same sources give the same bytes.
#
#   build/make-sample-packs.sh [--no-models]
# Signing: set LBN_SIGNING_KEY=<minisign secret key file> to sign every pack.json and the catalog.
# Models: the GGUF files are downloaded once into .dev-cache/models (pinned revision + SHA-256) if missing.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${OUT:-$ROOT/dist/packs}"
CLI="${LBN_CLI:-$ROOT/target/release/lastbook}"
[ -x "$CLI" ] || cargo build -q --release -p lbn-cli
MODELS=1; [ "${1:-}" = "--no-models" ] && MODELS=0

fetch_model() { # repo revision file sha256
  local dst=".dev-cache/models/$3"
  mkdir -p .dev-cache/models
  if [ ! -f "$dst" ] || ! echo "$4  $dst" | sha256sum -c --quiet 2>/dev/null; then
    curl -sSfL -C - -o "$dst" "https://huggingface.co/$1/resolve/$2/$3"
    echo "$4  $dst" | sha256sum -c --quiet
  fi
}

rm -rf "$OUT"; mkdir -p "$OUT"
build() { # src-dir id
  local src="$1" id="$2"
  "$CLI" pack build "$src" "$OUT/$id" ${LBN_SIGNING_KEY:+--sign "$LBN_SIGNING_KEY"} >/dev/null
  "$CLI" pack zip "$OUT/$id" "$OUT/$id.lbpack" >/dev/null
  echo "built $id"
}

# The PDF and its page text are generated (tools/make_pdf.py, pdf.js); the video needs ffmpeg (see the build container).
python3 tools/make_pdf.py packs/sample/docs/res/supplies-checklist.pages.src.json packs/sample/docs/res/supplies-checklist.pdf >/dev/null
if command -v node >/dev/null && [ -d apps/web/node_modules/pdfjs-dist ]; then
  node apps/web/tools/pdf-pages.mjs packs/sample/docs/res/supplies-checklist.pdf packs/sample/docs/res/supplies-checklist.pages.json 2>/dev/null >/dev/null
fi
build packs/sample/docs org.lastbook.sample.docs
build packs/sample/video org.lastbook.sample.video
build packs/sample/map org.lastbook.sample.map
build packs/sample/encyclopedia org.lastbook.sample.encyclopedia

if [ "$MODELS" = 1 ]; then
  fetch_model Qwen/Qwen2.5-1.5B-Instruct-GGUF 91cad51170dc346986eccefdc2dd33a9da36ead9 qwen2.5-1.5b-instruct-q4_k_m.gguf 6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e
  fetch_model Qwen/Qwen2.5-0.5B-Instruct-GGUF 9217f5db79a29953eb74d5343926648285ec7e67 qwen2.5-0.5b-instruct-q4_k_m.gguf 74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db
  for v in 1.5b 0.5b; do
    ln -f ".dev-cache/models/qwen2.5-$v-instruct-q4_k_m.gguf" "packs/models/qwen2.5-$v/model/" 2>/dev/null \
      || cp ".dev-cache/models/qwen2.5-$v-instruct-q4_k_m.gguf" "packs/models/qwen2.5-$v/model/"
    build "packs/models/qwen2.5-$v" "org.lastbook.model.qwen2.5-$v-instruct"
  done
fi

python3 - "$OUT" <<'PY'
import json, hashlib, os, sys
out = sys.argv[1]
packs = []
for d in sorted(os.listdir(out)):
    p = os.path.join(out, d, "pack.json")
    if not os.path.isfile(p):
        continue
    raw = open(p, "rb").read(); m = json.loads(raw)
    lb = os.path.join(out, d + ".lbpack")
    packs.append({"id": m["id"], "version": m["version"], "kind": m["kind"], "title": m["title"], "description": m["description"],
                  "license": m["license"]["spdx"], "bytes": sum(f["size"] for f in m["files"]),
                  "manifest": f"{d}/pack.json", "manifest_sha256": hashlib.sha256(raw).hexdigest(),
                  "lbpack": f"{d}.lbpack", "lbpack_bytes": os.path.getsize(lb),
                  "lbpack_sha256": hashlib.sha256(open(lb, "rb").read()).hexdigest()})
cat = {"format": "lastbook-catalog", "format_version": 1, "title": "Lastbook Next sample packs",
       "published": "2026-11-17", "expires": "2027-11-17", "packs": packs}
open(os.path.join(out, "catalog.json"), "w").write(json.dumps(cat, indent=1) + "\n")
with open(os.path.join(out, "SHA256SUMS"), "w") as f:
    for d in sorted(os.listdir(out)):
        if d.endswith(".lbpack") or d == "catalog.json":
            f.write(f"{hashlib.sha256(open(os.path.join(out, d), 'rb').read()).hexdigest()}  {d}\n")
print(f"catalog: {len(packs)} packs")
PY
if [ -n "${LBN_SIGNING_KEY:-}" ]; then
  # The catalog is signed with the same publisher key (minisign over the exact bytes).
  "$CLI" pack sign-catalog "$OUT/catalog.json" "$LBN_SIGNING_KEY"
fi
du -sh "$OUT"
