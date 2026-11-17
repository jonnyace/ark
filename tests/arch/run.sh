#!/usr/bin/env bash
# Run the offline desktop journey against the AppImage on Arch Linux (Omarchy's base) with networking disabled.
# Needs a library prepared by the desktop journey's install phase (.dev-cache/e2e/desktop-data) and tauri-driver in
# .dev-cache/container-cargo/bin (installed by tests/e2e/desktop/run.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
docker build -q -t lastbook-ark-arch-test:dev tests/arch >/dev/null
APP="$(ls dist/desktop/lastbook-ark_*_x86_64.AppImage | head -1)"
docker run --rm --network none -u "$(id -u):$(id -g)" -v /etc/passwd:/etc/passwd:ro -v "$ROOT:/src" \
  -v "$ROOT/.dev-cache/container-cargo/bin/tauri-driver:/usr/local/bin/tauri-driver:ro" -w /src \
  -e HOME=/tmp -e LBN_DATA=${ARCH_DATA:-/src/.dev-cache/e2e/desktop-data} lastbook-ark-arch-test:dev bash -c "
  export XDG_RUNTIME_DIR=/tmp/xdg APPIMAGE_EXTRACT_AND_RUN=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
  mkdir -p \$XDG_RUNTIME_DIR; chmod 700 \$XDG_RUNTIME_DIR
  Xvfb :99 -screen 0 1280x900x24 >/tmp/xvfb.log 2>&1 & export DISPLAY=:99
  eval \$(dbus-launch --sh-syntax); sleep 1
  tauri-driver --port 4444 --native-driver /usr/bin/WebKitWebDriver >/tmp/td.log 2>&1 &
  for i in \$(seq 50); do curl -s localhost:4444/status >/dev/null 2>&1 && break; sleep 0.2; done
  cat /etc/os-release | head -1
  node ${ARCH_SCRIPT:-tests/e2e/desktop/journey.mjs} /src/$APP ${ARCH_ARGS:-offline /src/.dev-cache/e2e} || { tail -20 /tmp/td.log; exit 1; }"
