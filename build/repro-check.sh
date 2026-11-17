#!/usr/bin/env bash
# Build each distributable twice, independently, and compare bytes. Prints REPRODUCIBLE / DIFFERS per artefact.
# Unsigned payloads only: signatures are made afterwards by build/release.sh and are not part of the comparison.
#   build/repro-check.sh [--desktop]     (--desktop also rebuilds the .deb twice in the container, ~3 min)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
[ -s "$HOME/.nvm/nvm.sh" ] && source "$HOME/.nvm/nvm.sh" >/dev/null
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
report() { if [ "$2" = "$3" ]; then echo "REPRODUCIBLE  $1  $2"; else echo "DIFFERS       $1  $2 vs $3"; fi; }
h() { sha256sum "$1" | cut -c1-16; }

# Web app (PWA static files): two clean builds.
for i in 1 2; do (cd apps/web && rm -rf dist && npx vite build >/dev/null 2>&1); tar --sort=name --mtime=@0 --owner=0 --group=0 -C apps/web -cf "$W/web$i.tar" dist; done
report "web app (all files)" "$(h "$W/web1.tar")" "$(h "$W/web2.tar")"

# Content packs: build each from source twice, compare pack.json and .lbpack.
CLI=target/release/lastbook
for p in docs video map encyclopedia; do
  for i in 1 2; do "$CLI" pack build packs/sample/$p "$W/$p$i" >/dev/null && "$CLI" pack zip "$W/$p$i" "$W/$p$i.lbpack" >/dev/null; done
  report "pack $p (.lbpack)" "$(h "$W/${p}1.lbpack")" "$(h "$W/${p}2.lbpack")"
done

# CLI binary: two builds in the pinned container with separate target directories.
for i in 1 2; do LBN_ENV=CARGO_TARGET_DIR=/src/.dev-cache/repro-target-$i build/in-container.sh "cargo build -q --release --locked -p lbn-cli" >/dev/null 2>&1; done
report "lastbook CLI (container)" "$(h .dev-cache/repro-target-1/release/lastbook)" "$(h .dev-cache/repro-target-2/release/lastbook)"
# wasm module
for i in 1 2; do LBN_ENV=CARGO_TARGET_DIR=/src/.dev-cache/repro-target-$i build/in-container.sh "cargo build -q --release --locked -p lbn-wasm --target wasm32-unknown-unknown" >/dev/null 2>&1; done
report "lbn-wasm module (container)" "$(h .dev-cache/repro-target-1/wasm32-unknown-unknown/release/lbn_wasm.wasm)" "$(h .dev-cache/repro-target-2/wasm32-unknown-unknown/release/lbn_wasm.wasm)"

if [ "${1:-}" = "--desktop" ]; then
  cp dist/desktop/lastbook-ark_*_amd64.deb "$W/a.deb"; cp dist/desktop/lastbook-ark "$W/a.bin"
  build/desktop.sh >/dev/null 2>&1
  report "desktop binary" "$(h "$W/a.bin")" "$(h dist/desktop/lastbook-ark)"
  report "desktop .deb" "$(h "$W/a.deb")" "$(h dist/desktop/lastbook-ark_*_amd64.deb)"
fi
