#!/usr/bin/env bash
# Run a command inside the pinned build container with this repository mounted at /src, as the current user.
# Cargo's download cache and pnpm's store are shared with the host so repeated builds do not download again.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE="${LBN_IMAGE:-lastbook-ark-build:dev}"
docker image inspect "$IMAGE" >/dev/null 2>&1 || docker build -t "$IMAGE" "$ROOT/build/docker"
mkdir -p "$HOME/.cargo/registry" "$HOME/.cargo/git" "$ROOT/.dev-cache/container-home" "$ROOT/.dev-cache/container-cargo"
NET=(); [ -n "${LBN_NETWORK:-}" ] && NET=(--network "$LBN_NETWORK")
# LBN_GPU=1 passes the host GPU (/dev/dri) into the container for the optional Vulkan inference path.
GPU=(); [ "${LBN_GPU_DEVICE:-0}" = 1 ] && GPU=(--device /dev/dri --group-add "$(getent group render | cut -d: -f3)" --group-add "$(getent group video | cut -d: -f3)")
exec docker run --rm -i ${LBN_TTY:+-t} "${NET[@]}" "${GPU[@]}" -u "$(id -u):$(id -g)" \
  -e HOME=/src/.dev-cache/container-home -e CARGO_HOME=/src/.dev-cache/container-cargo -e RUSTUP_HOME=/opt/rustup \
  -e PATH=/src/.dev-cache/container-cargo/bin:/opt/cargo/bin:/opt/node/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  -e CARGO_TARGET_DIR=/src/.dev-cache/container-target ${LBN_ENV:+-e "$LBN_ENV"} \
  -v /etc/passwd:/etc/passwd:ro -v /etc/group:/etc/group:ro -v "$ROOT:/src" -v "$HOME/.cargo/registry:/src/.dev-cache/container-cargo/registry" -v "$HOME/.cargo/git:/src/.dev-cache/container-cargo/git" \
  ${LBN_EXTRA_MOUNT:+-v "$LBN_EXTRA_MOUNT"} -w /src "$IMAGE" bash -c "$*"
