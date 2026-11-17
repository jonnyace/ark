#!/usr/bin/env bash
# Clean-machine installation test (criterion 1): a plain Debian 13 image (not the build image), the .deb installed
# with apt exactly as HANDOFF.md tells a person to, then the full desktop journey against /usr/bin/lastbook-ark:
#   1. online:  apt install ./lastbook-ark.deb; personal data of schema 1 seeded (an "older app" left it);
#               install packs over HTTP from a local server and the AI model from a .lbpack file.
#   2. offline: the same machine image with --network none: restart, search, read, video, map, AI + citations;
#               the old bookmark was migrated; `lastbook export` (installed CLI) writes the personal data.
# Test tooling (Xvfb, WebKitWebDriver, Node, tauri-driver) is added only for driving the window.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
DEB="$(ls dist/desktop/*.deb | head -1)"
BASE=debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
NODE="$(dirname "$(dirname "$(readlink -f "$(command -v node)")")")"   # the Node installation on PATH
OUT="$ROOT/.dev-cache/e2e/clean"; rm -rf "$OUT"; mkdir -p "$OUT"
NAME=lbn-clean-$$
MOUNTS=(-v "$ROOT:/src:ro" -v "$OUT:/out" -v "$NODE:/opt/node:ro" -v "$ROOT/.dev-cache/container-cargo/bin/tauri-driver:/usr/local/bin/tauri-driver:ro")
DRIVE='export HOME=/root XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
  export WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1 PATH=/opt/node/bin:$PATH LBN_DATA=/root/.local/share/org.lastbook.ark
  Xvfb :99 -screen 0 1280x900x24 >/tmp/xvfb.log 2>&1 & export DISPLAY=:99
  eval $(dbus-launch --sh-syntax); sleep 1
  tauri-driver --port 4444 --native-driver /usr/bin/WebKitWebDriver >/tmp/td.log 2>&1 &
  for i in $(seq 50); do curl -s localhost:4444/status >/dev/null 2>&1 && break; sleep 0.2; done'

echo "== phase 1: install the package on a clean Debian 13 and set up a library"
docker run --name "$NAME" "${MOUNTS[@]}" "$BASE" bash -c "
  set -e
  apt-get update -qq
  # The person's step (HANDOFF.md): apt resolves WebKitGTK, GStreamer and the rest.
  apt-get install -y -qq /src/$DEB > /out/apt-install.log 2>&1
  dpkg -s lastbook-ark | grep -E '^(Status|Version)' > /out/dpkg-status.txt
  ls -la '/usr/lib/Lastbook Ark/runner/lastbook-llm' /usr/bin/lastbook /usr/bin/lastbook-ark >> /out/dpkg-status.txt
  # Test tooling only:
  apt-get install -y -qq --no-install-recommends xvfb xauth dbus-x11 webkit2gtk-driver curl ca-certificates sqlite3 >/dev/null 2>&1
  # An older app (personal schema 1) left a bookmark behind.
  mkdir -p /root/.local/share/org.lastbook.ark
  sqlite3 /root/.local/share/org.lastbook.ark/personal.sqlite \"CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL); INSERT INTO meta VALUES('schema','1');
    CREATE TABLE bookmarks(id TEXT PRIMARY KEY, pack TEXT NOT NULL, resource TEXT NOT NULL, page INTEGER, title TEXT NOT NULL, created TEXT NOT NULL);
    INSERT INTO bookmarks VALUES('old1','org.lastbook.sample.docs','supplies-checklist',3,'Kit upkeep (from the old app)','2026-01-01T00:00:00Z');
    CREATE TABLE history(n INTEGER PRIMARY KEY AUTOINCREMENT, target TEXT NOT NULL, title TEXT NOT NULL, at TEXT NOT NULL);
    CREATE TABLE settings(k TEXT PRIMARY KEY, v TEXT NOT NULL);\"
  $DRIVE
  cd /src/tests/e2e
  node server.mjs 18787 /src/apps/web/dist /src/dist/packs >/tmp/srv.log 2>&1 &
  sleep 1
  node desktop/journey.mjs /usr/bin/lastbook-ark install /out
" 2>&1 | tail -12
docker commit "$NAME" lbn-clean-installed:latest >/dev/null; docker rm "$NAME" >/dev/null

echo "== phase 2: same machine, no network"
docker run --rm --network none "${MOUNTS[@]}" lbn-clean-installed:latest bash -c "
  set -e
  $DRIVE
  cd /src/tests/e2e
  node desktop/journey.mjs /usr/bin/lastbook-ark offline /out
  node desktop/saved.mjs /usr/bin/lastbook-ark /out
  lastbook --data \$LBN_DATA export /out/personal-export.json
  ls /root/.local/share/org.lastbook.ark/backups/
" 2>&1 | tail -30
docker image rm lbn-clean-installed:latest >/dev/null
