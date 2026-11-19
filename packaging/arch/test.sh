#!/usr/bin/env bash
# Build the Arch package from the current commit in a clean Arch container, install it with pacman, and run:
#   install journey (packs over HTTP from a local server), offline journey (--network none), AV1/H.264 playback.
# Results and the built package go to .dev-cache/arch-pkg. Needs: dist/packs (build/make-sample-packs.sh), the desktop
# test data from tests/e2e/desktop (tauri-driver in .dev-cache/container-cargo/bin), and optionally an imported legacy
# library in .dev-cache/e2e/legacy-lib for the codec test.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
VER=$(sed -n 's/^_ver=//p' packaging/arch/PKGBUILD)
OUT="$ROOT/.dev-cache/arch-pkg"; rm -rf "$OUT"; mkdir -p "$OUT"
# Same layout as GitHub's tag archive, which the PKGBUILD downloads: one top folder named after the repository.
# Its bytes differ from GitHub's, so makepkg below skips the PKGBUILD's checksum (which is for the published tag).
git archive --prefix="ark-$VER/" -o "$OUT/ark-library-$VER.tar.gz" HEAD
cp packaging/arch/PKGBUILD "$OUT/"
docker build -q -t ark-library-arch-build:dev -f packaging/arch/test.Dockerfile packaging/arch >/dev/null
TD="$ROOT/.dev-cache/container-cargo/bin/tauri-driver"
MOUNTS=(-v "$ROOT:/src:ro" -v "$OUT:/out" -v "$TD:/usr/local/bin/tauri-driver:ro" -v "$ROOT/.dev-cache/e2e:/e2e")
DRIVE='export XDG_RUNTIME_DIR=/tmp/xdg WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
  mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
  Xvfb :99 -screen 0 1280x900x24 >/tmp/xvfb.log 2>&1 & export DISPLAY=:99
  eval $(dbus-launch --sh-syntax); sleep 1
  tauri-driver --port 4444 --native-driver /usr/bin/WebKitWebDriver >/tmp/td.log 2>&1 &
  for i in $(seq 50); do curl -s localhost:4444/status >/dev/null 2>&1 && break; sleep 0.2; done'
NAME=lbn-arch-$$
echo "== build with makepkg, install with pacman, install journey"
docker run --name "$NAME" "${MOUNTS[@]}" ark-library-arch-build:dev bash -c "
  set -e
  mkdir /build && cp /out/PKGBUILD /out/ark-library-$VER.tar.gz /build/ && chown -R builder /build
  su builder -c 'cd /build && makepkg --noconfirm --nodeps --skipchecksums 2>&1' > /out/makepkg.log 2>&1 || { tail -40 /out/makepkg.log; exit 1; }
  grep -E 'Finished making|test result' /out/makepkg.log | tail -6
  cp /build/*.pkg.tar.zst /out/
  # The test image is cached, so its package database ages: bring the system to today's repository first (a full -Syu;
  # Arch does not support partial upgrades), or pacman asks the mirrors for package versions they no longer have.
  pacman -Syu --noconfirm > /out/pacman-sync.log 2>&1 || { tail -20 /out/pacman-sync.log; exit 1; }
  pacman -U --noconfirm /out/ark-library-*.pkg.tar.zst > /out/pacman.log 2>&1 || { tail -20 /out/pacman.log; exit 1; }
  pacman -Q ark-library gst-plugin-dav1d gst-libav
  ls /usr/lib/ark-library/runner/lastbook-llm
  export LBN_DATA=/root/lbn-data
  $DRIVE
  cd /src/tests/e2e
  node server.mjs 18787 /src/apps/web/dist /src/dist/packs >/tmp/srv.log 2>&1 &
  sleep 1
  node desktop/journey.mjs /usr/bin/ark-library install /out
"
docker commit "$NAME" lbn-arch-installed:latest >/dev/null; docker rm "$NAME" >/dev/null
echo "== same machine, no network: offline journey + AV1/H.264 playback"
docker run --rm --network none "${MOUNTS[@]}" lbn-arch-installed:latest bash -c "
  set -e
  export LBN_DATA=/root/lbn-data
  $DRIVE
  cd /src/tests/e2e
  node desktop/journey.mjs /usr/bin/ark-library offline /out
"
# The app inherits its environment from tauri-driver, so LBN_DATA must be set before the driver starts.
if [ -d .dev-cache/e2e/legacy-lib ] && [ -f .dev-cache/e2e/legacy-videos.txt ]; then
  docker run --rm --network none "${MOUNTS[@]}" lbn-arch-installed:latest bash -c "
    set -e
    export LBN_DATA=/e2e/legacy-lib
    $DRIVE
    cd /src/tests/e2e
    node desktop/codecs.mjs /usr/bin/ark-library org.lastbook.legacy.small /e2e/legacy-videos.txt
  "
fi
docker image rm lbn-arch-installed:latest >/dev/null
ls -la "$OUT"/*.pkg.tar.zst
