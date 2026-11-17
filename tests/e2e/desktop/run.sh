#!/usr/bin/env bash
# Run a desktop WebDriver script inside the build container: Xvfb + dbus + tauri-driver (WebKitWebDriver).
# Usage (from repo root): tests/e2e/desktop/run.sh <script.mjs> [args...]
#   LBN_NETWORK=none  run with networking disabled for the app and the test (offline test).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"; cd "$ROOT"
SCRIPT="$1"; shift
exec build/in-container.sh "
  set -e
  export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p \$XDG_RUNTIME_DIR; chmod 700 \$XDG_RUNTIME_DIR
  export WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
  Xvfb :99 -screen 0 1280x900x24 >/tmp/xvfb.log 2>&1 & export DISPLAY=:99
  eval \$(dbus-launch --sh-syntax)
  sleep 1
  tauri-driver --port 4444 --native-driver /usr/bin/WebKitWebDriver >/tmp/tauri-driver.log 2>&1 &
  for i in \$(seq 50); do curl -s localhost:4444/status >/dev/null 2>&1 && break; sleep 0.2; done
  node $SCRIPT $* ; rc=\$?
  [ \$rc = 0 ] || tail -30 /tmp/tauri-driver.log
  exit \$rc
"
