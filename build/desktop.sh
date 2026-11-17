#!/usr/bin/env bash
# Build the Linux desktop app (Tauri) inside the build container: web UI, llama.cpp runner, then the Tauri bundles.
#   build/desktop.sh            -> dist/desktop/ (binary, .deb, .AppImage if bundling succeeds)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
build/in-container.sh '
  set -euo pipefail
  [ -x .dev-cache/runner-bin/lastbook-llm ] || W=.dev-cache runner/build.sh
  build/gen-ts.sh >/dev/null
  build/build-wasm.sh
  (cd apps/web && npx vite build >/dev/null)
  cd apps/desktop
  APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 npx tauri build --config src-tauri/tauri.conf.json 2>&1 | tail -20
'
mkdir -p dist/desktop
B=.dev-cache/container-target/release
cp "$B/lastbook-next" dist/desktop/
rm -rf dist/desktop/runner && cp -r .dev-cache/runner-bin dist/desktop/runner   # found beside the binary when run unpacked
cp $B/bundle/deb/*.deb dist/desktop/ 2>/dev/null || true
cp $B/bundle/appimage/*.AppImage dist/desktop/ 2>/dev/null || true
ls -la dist/desktop
