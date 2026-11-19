#!/usr/bin/env bash
# All automated checks, runnable on any machine (CI only orchestrates this script).
#   build/ci.sh                 fast checks: format, lint, unit + integration tests, type checks, web build, TS drift
#   build/ci.sh --e2e           + browser suite (Chromium, Firefox, WebKit; needs `npx playwright install`)
#   build/ci.sh --desktop       + desktop build in the container and the desktop journey (install + offline)
#   build/ci.sh --clean-install + .deb installed on a clean Debian 13 image and the offline journey (needs --desktop artefacts)
#   build/ci.sh --ai            + real-model grounded answer check (needs model packs: build/make-sample-packs.sh)
#   build/ci.sh --gpu           + Vulkan runner build and a GPU inference probe (needs /dev/dri; report only)
#   build/ci.sh --all           everything
# --e2e re-signs test fixtures: set LBN_SIGNING_KEY or run through build/with-signing-key.sh.
# Requirements: Rust (rust-toolchain.toml), Node 22 + pnpm, Docker for --desktop/--clean-install.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
E2E=0; DESKTOP=0; CLEAN=0; AI=0; GPU=0
for a in "$@"; do case "$a" in
  --e2e) E2E=1;; --desktop) DESKTOP=1;; --clean-install) CLEAN=1;; --ai) AI=1;; --gpu) GPU=1;; --all) E2E=1; DESKTOP=1; CLEAN=1; AI=1;;
  *) echo "unknown option $a" >&2; exit 2;; esac; done
[ -s "$HOME/.nvm/nvm.sh" ] && source "$HOME/.nvm/nvm.sh" >/dev/null
step() { printf '\n== %s\n' "$*"; }

step "One version everywhere";  build/check-versions.sh
step "Rust format";             cargo fmt --all --check
step "Rust lint";               cargo clippy -q --workspace --all-targets --locked -- -D warnings
step "Rust tests";              cargo test -q --workspace --locked 2>&1 | grep -E "test result|FAILED|panicked" ; test "${PIPESTATUS[0]}" = 0
step "Core builds for wasm32";  cargo build -q -p lbn-core --target wasm32-unknown-unknown --locked
step "TypeScript types up to date"; build/gen-ts.sh >/dev/null; git diff --exit-code -- apps/web/src/lib/engine/gen
step "Web dependencies";        pnpm install --frozen-lockfile >/dev/null
step "WASM bindings";           build/build-wasm.sh
step "Svelte/TypeScript check"; (cd apps/web && npx svelte-check --tsconfig ./tsconfig.json --output human | tail -1)
step "Web build";               (cd apps/web && npx vite build | tail -1)
step "Sample packs (without models)"
[ -d dist/packs/org.lastbook.sample.docs ] || build/make-sample-packs.sh --no-models
if [ "$E2E" = 1 ]; then
  # Browser profiles hold whole model packs (1.1 GB): keep them on disk, not in a RAM-backed /tmp.
  mkdir -p .dev-cache/tmp
  step "Browser end-to-end";    tests/e2e/make-test-packs.sh; (cd tests/e2e && TMPDIR="$ROOT/.dev-cache/tmp" npx playwright test)
  if command -v xvfb-run >/dev/null; then
    step "Firefox with a display (WebGL map)"
    (cd tests/e2e && LBN_HEADED=1 TMPDIR="$ROOT/.dev-cache/tmp" xvfb-run -a -s "-screen 0 1280x1000x24" npx playwright test journey offline --project firefox)
  fi
fi
if [ "$DESKTOP" = 1 ]; then
  step "Desktop build (container)"; build/desktop.sh
  step "Desktop journey"
  rm -rf .dev-cache/e2e/desktop-data
  LBN_ENV=LBN_DATA=/src/.dev-cache/e2e/desktop-data tests/e2e/desktop/run.sh \
    "tests/e2e/server.mjs 18787 /src/apps/web/dist /src/dist/packs > /tmp/srv.log 2>&1 & sleep 1; node tests/e2e/desktop/journey.mjs" \
    /src/dist/desktop/ark-library install /src/.dev-cache/e2e
  LBN_NETWORK=none LBN_ENV=LBN_DATA=/src/.dev-cache/e2e/desktop-data tests/e2e/desktop/run.sh tests/e2e/desktop/journey.mjs \
    /src/dist/desktop/ark-library offline /src/.dev-cache/e2e
  step "AppImage, offline"
  LBN_NETWORK=none LBN_ENV=LBN_DATA=/src/.dev-cache/e2e/desktop-data tests/e2e/desktop/run.sh tests/e2e/desktop/journey.mjs \
    "/src/dist/desktop/ark-library_$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)_x86_64.AppImage" offline /src/.dev-cache/e2e
fi
if [ "$CLEAN" = 1 ]; then
  step "Clean-machine install";  tests/clean-install/run.sh
fi
if [ "$AI" = 1 ]; then
  step "Grounded AI answers (real model)"
  D=$(mktemp -d); export LBN_DATA="$D" LBN_RUNNER="$ROOT/.dev-cache/runner-bin/lastbook-llm"
  for p in docs video encyclopedia; do target/release/ark-pack install "dist/packs/org.lastbook.sample.$p.lbpack" >/dev/null; done
  target/release/ark-pack install dist/packs/org.lastbook.model.qwen2.5-1.5b-instruct.lbpack >/dev/null
  target/release/ark-pack ai --enable >/dev/null
  python3 tools/ai_eval.py target/release/ark-pack
  rm -rf "$D"
fi
if [ "$GPU" = 1 ]; then
  step "Vulkan runner + GPU probe (report only)"
  LBN_GPU_DEVICE=1 build/in-container.sh "vulkaninfo --summary 2>/dev/null | grep deviceName; W=.dev-cache/vk LBN_VULKAN=1 runner/build.sh | tail -1;
    python3 tools/runner_probe.py .dev-cache/vk/runner-bin/lastbook-llm .dev-cache/models/qwen2.5-1.5b-instruct-q4_k_m.gguf --gpu | tail -2"
fi
step "All requested checks passed"
