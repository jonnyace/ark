#!/usr/bin/env bash
# Check out llama.cpp at the pinned tag and commit into $1 (default .dev-cache/llama.cpp).
set -euo pipefail
D="${1:-.dev-cache/llama.cpp}"; TAG=b12032; COMMIT=dde879bc4f954b809892fd0d660515f42536394a
[ -d "$D/.git" ] || git -c advice.detachedHead=false clone -q --depth 1 --branch "$TAG" https://github.com/ggml-org/llama.cpp.git "$D"
[ "$(git -C "$D" rev-parse HEAD)" = "$COMMIT" ] || { echo "llama.cpp is not at $TAG ($COMMIT)" >&2; exit 1; }
echo "$D"
