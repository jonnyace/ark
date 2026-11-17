#!/usr/bin/env bash
# Build the Linux desktop app (Tauri) inside the build container: web UI, llama.cpp runner, then the Tauri bundles.
#   build/desktop.sh            -> dist/desktop/ (binary, .deb, .AppImage if bundling succeeds)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
build/in-container.sh '
  set -euo pipefail
  [ -x .dev-cache/runner-bin/lastbook-llm ] || W=.dev-cache runner/build.sh
  cargo build -q --release --locked -p lbn-cli
  build/gen-ts.sh >/dev/null
  build/build-wasm.sh
  (cd apps/web && npx vite build >/dev/null)
  cd apps/desktop
  APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 npx tauri build --config src-tauri/tauri.conf.json 2>&1 | tail -20
  # Re-pack the .deb deterministically: same files, fixed modification times, root ownership (the bundler stamps the
  # build time into every entry, so two identical builds gave different packages).
  cd "$CARGO_TARGET_DIR/release/bundle/deb"
  D=$(ls *.deb | head -1); rm -rf repack && dpkg-deb -R "$D" repack
  find repack -exec touch -h -d "2026-01-01 00:00:00Z" {} +
  SOURCE_DATE_EPOCH=1767225600 dpkg-deb --root-owner-group -Zxz --build repack "$D" >/dev/null && rm -rf repack
'
mkdir -p dist/desktop
B=.dev-cache/container-target/release
cp "$B/lastbook-next" "$B/lastbook" dist/desktop/
rm -rf dist/desktop/runner && cp -r .dev-cache/runner-bin dist/desktop/runner   # found beside the binary when run unpacked
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' apps/desktop/src-tauri/Cargo.toml | head -1)
rm -f dist/desktop/*.deb dist/desktop/*.AppImage
cp "$B"/bundle/deb/*.deb "dist/desktop/lastbook-next_${VER}_amd64.deb"
cp "$B"/bundle/appimage/*.AppImage "dist/desktop/lastbook-next_${VER}_x86_64.AppImage" 2>/dev/null || true
ls -la dist/desktop
