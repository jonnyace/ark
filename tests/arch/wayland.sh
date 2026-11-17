#!/usr/bin/env bash
# Run a desktop test script with Lastbook Ark as a pure Wayland client on Arch: no X server, DISPLAY unset,
# GDK_BACKEND=wayland. COMPOSITOR=sway (default; headless wlroots) or COMPOSITOR=hyprland (Hyprland nested in headless
# sway, software rendering). Offline (--network none).
#   tests/arch/wayland.sh <app path inside /src> [script] [args...]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
docker build -q -t lastbook-ark-arch-test:dev tests/arch >/dev/null
docker build -q -t lastbook-ark-arch-wayland:dev -f tests/arch/wayland.Dockerfile tests/arch >/dev/null
APP="$1"; shift; SCRIPT="${1:-tests/e2e/desktop/journey.mjs}"; shift || true; ARGS="${*:-offline /src/.dev-cache/e2e}"
docker run --rm --network none -u "$(id -u):$(id -g)" -v /etc/passwd:/etc/passwd:ro -v "$ROOT:/src" \
  -v "$ROOT/.dev-cache/container-cargo/bin/tauri-driver:/usr/local/bin/tauri-driver:ro" -w /src \
  -e HOME=/tmp -e LBN_DATA="${LBN_DATA:-/src/.dev-cache/e2e/desktop-data}" -e COMPOSITOR="${COMPOSITOR:-sway}" \
  lastbook-ark-arch-wayland:dev bash -c "
  export XDG_RUNTIME_DIR=/tmp/xdg APPIMAGE_EXTRACT_AND_RUN=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
  mkdir -p \$XDG_RUNTIME_DIR; chmod 700 \$XDG_RUNTIME_DIR; unset DISPLAY
  eval \$(dbus-launch --sh-syntax)
  printf 'output HEADLESS-1 resolution 1280x900\n' > /tmp/sway.conf
  # Arch installs sway with a file capability that an unprivileged container user may not exec; a plain copy works.
  cp /usr/bin/sway /tmp/sway-nocap
  WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 /tmp/sway-nocap -c /tmp/sway.conf >/tmp/sway.log 2>&1 &
  for i in \$(seq 50); do ls \$XDG_RUNTIME_DIR/wayland-* >/dev/null 2>&1 && break; sleep 0.2; done
  export WAYLAND_DISPLAY=\$(basename \$(ls \$XDG_RUNTIME_DIR/wayland-* | grep -v lock | head -1))
  if [ \"\$COMPOSITOR\" = hyprland ]; then
    printf 'misc {\n disable_hyprland_logo = true\n disable_splash_rendering = true\n}\n' > /tmp/hypr.conf
    Hyprland -c /tmp/hypr.conf >/tmp/hypr.log 2>&1 &
    for i in \$(seq 100); do [ \$(ls \$XDG_RUNTIME_DIR/wayland-* 2>/dev/null | grep -v lock | wc -l) -ge 2 ] && break; sleep 0.2; done
    export WAYLAND_DISPLAY=\$(ls -t \$XDG_RUNTIME_DIR/wayland-* | grep -v lock | head -1 | xargs basename)
    pgrep -x Hyprland >/dev/null || { echo 'Hyprland did not start:'; tail -15 /tmp/hypr.log; exit 2; }
  fi
  echo \"compositor=\$COMPOSITOR WAYLAND_DISPLAY=\$WAYLAND_DISPLAY DISPLAY=\${DISPLAY:-unset}\"
  export GDK_BACKEND=wayland
  tauri-driver --port 4444 --native-driver /usr/bin/WebKitWebDriver >/tmp/td.log 2>&1 &
  for i in \$(seq 50); do curl -s localhost:4444/status >/dev/null 2>&1 && break; sleep 0.2; done
  node $SCRIPT /src/$APP $ARGS || { tail -20 /tmp/td.log; tail -10 /tmp/sway.log; exit 1; }"
