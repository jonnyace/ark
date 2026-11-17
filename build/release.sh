#!/usr/bin/env bash
# Assemble a release candidate in dist/release/<version>/ from already-built outputs:
#   desktop (.deb, .AppImage), the lastbook CLI, the PWA as a static site archive, the llama.cpp runner, sample and
#   model packs (.lbpack + catalog), licence inventory, build information, SHA256SUMS (+ minisign signature if
#   LBN_SIGNING_KEY is set). Archives are deterministic (sorted, fixed times/owners, gzip -n).
# Run after: build/desktop.sh, (cd apps/web && npx vite build), build/make-sample-packs.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OUT="dist/release/$VER"; rm -rf "$OUT"; mkdir -p "$OUT"
tarit() { # out.tar.gz dir [transform-name]
  tar --sort=name --mtime='2026-01-01 00:00:00Z' --owner=0 --group=0 --numeric-owner --pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime \
      -C "$(dirname "$2")" -cf - "$(basename "$2")" | gzip -n -9 > "$1"
}
cp dist/desktop/lastbook-next_${VER}_amd64.deb "$OUT/"
cp dist/desktop/lastbook-next_${VER}_x86_64.AppImage "$OUT/" 2>/dev/null || echo "no AppImage built"
T=$(mktemp -d)
mkdir -p "$T/lastbook-cli-$VER-linux-x86_64" && cp dist/desktop/lastbook LICENSE NOTICE.md "$T/lastbook-cli-$VER-linux-x86_64/"
tarit "$OUT/lastbook-cli-$VER-linux-x86_64.tar.gz" "$T/lastbook-cli-$VER-linux-x86_64"
cp -r apps/web/dist "$T/lastbook-next-web-$VER" && cp LICENSE NOTICE.md docs/LICENSES.md "$T/lastbook-next-web-$VER/"
tarit "$OUT/lastbook-next-web-$VER.tar.gz" "$T/lastbook-next-web-$VER"
cp -r .dev-cache/runner-bin "$T/lastbook-llm-b12032-linux-x86_64"
tarit "$OUT/lastbook-llm-b12032-linux-x86_64.tar.gz" "$T/lastbook-llm-b12032-linux-x86_64"
rm -rf "$T"
mkdir -p "$OUT/packs"
cp dist/packs/*.lbpack dist/packs/catalog.json "$OUT/packs/"
cp dist/packs/catalog.json.minisig "$OUT/packs/" 2>/dev/null || true
cp docs/LICENSES.md "$OUT/THIRD-PARTY-LICENSES.md"; cp LICENSE NOTICE.md "$OUT/"
cat > "$OUT/BUILD-INFO.json" <<JSON
{
  "version": "$VER",
  "git_commit": "$(git rev-parse HEAD)",
  "git_dirty": $( [ -z "$(git status --porcelain -- crates apps runner build packs keys)" ] && echo false || echo true ),
  "rust": "$(rustc --version)",
  "node": "$(node --version 2>/dev/null)",
  "build_image": "debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2 + build/docker/Dockerfile",
  "llama_cpp": "b12032 (dde879bc4f954b809892fd0d660515f42536394a)",
  "signing": "minisign key named in keys/trusted.json (development sample publisher)"
}
JSON
(cd "$OUT" && find . -type f ! -name SHA256SUMS ! -name '*.minisig' -printf '%P\n' | sort | xargs sha256sum > SHA256SUMS)
if [ -n "${LBN_SIGNING_KEY:-}" ]; then
  # minisign signature over SHA256SUMS: verify with `minisign -Vm SHA256SUMS -p keys/sample-publisher.pub`
  target/release/lastbook pack sign-file "$OUT/SHA256SUMS" "$LBN_SIGNING_KEY"
fi
du -sh "$OUT"; cat "$OUT/SHA256SUMS"
