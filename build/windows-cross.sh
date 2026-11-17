#!/usr/bin/env bash
# Cross-build the Windows x86-64 release from Linux and test what Wine can test.
#   build/windows-cross.sh          -> dist/windows/ (lastbook.exe, lastbook-ark.exe, NSIS setup, runner)
# Tests: all Rust tests compiled for Windows run under Wine; the Windows CLI installs/verifies/searches packs.
# NOT tested here: the desktop window (needs Microsoft WebView2, unavailable under Wine) and anything on real Windows.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
docker image inspect lastbook-ark-wincross:dev >/dev/null 2>&1 || docker build -t lastbook-ark-wincross:dev -f build/docker/windows-cross.Dockerfile build/docker
LBN_IMAGE=lastbook-ark-wincross:dev LBN_ENV=CARGO_TARGET_DIR=/src/.dev-cache/win-target build/in-container.sh '
  set -euo pipefail
  export WINEPREFIX=/src/.dev-cache/wine WINEDEBUG=-all XDG_RUNTIME_DIR=/tmp CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine
  runner/build-windows-cross.sh | tail -1
  cargo build -q --release --locked -p lbn-cli --target x86_64-pc-windows-gnu
  cargo test -q --locked --target x86_64-pc-windows-gnu -p lbn-core -p lbn-native 2>&1 | grep -E "test result|FAILED|panicked"
  rm -rf "$CARGO_TARGET_DIR/x86_64-pc-windows-gnu/release/bundle"   # no stale installers from earlier builds
  (cd apps/desktop && npx tauri build --target x86_64-pc-windows-gnu --bundles nsis 2>&1 | tail -2)
'
rm -rf dist/windows; mkdir -p dist/windows; T=.dev-cache/win-target/x86_64-pc-windows-gnu/release
cp $T/lastbook.exe $T/lastbook-ark.exe dist/windows/
cp "$T"/bundle/nsis/*-setup.exe dist/windows/lastbook-ark_0.9.0-rc.1_x64-setup-unsigned.exe
cp -r .dev-cache/bin-win dist/windows/runner
ls -la dist/windows
