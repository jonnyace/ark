#!/usr/bin/env bash
# Build lbn-wasm and generate its JS bindings into apps/web/src/lib/wasm (generated, not committed).
# wasm-bindgen-cli must match the wasm-bindgen crate version pinned in crates/lbn-wasm/Cargo.toml; it is installed
# into .dev-cache/tools if missing.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
WBG_VERSION="$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' crates/lbn-wasm/Cargo.toml)"
WBG="$ROOT/.dev-cache/tools/bin/wasm-bindgen"
if ! "$WBG" --version 2>/dev/null | grep -q "$WBG_VERSION"; then
  cargo install -q --locked --root "$ROOT/.dev-cache/tools" wasm-bindgen-cli --version "$WBG_VERSION"
fi
cargo build -q --release -p lbn-wasm --target wasm32-unknown-unknown
rm -rf apps/web/src/lib/wasm
"$WBG" --target web --out-dir apps/web/src/lib/wasm --out-name lbn "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/lbn_wasm.wasm"
echo "wasm: $(du -h apps/web/src/lib/wasm/lbn_bg.wasm | cut -f1)"
