#!/usr/bin/env bash
# Regenerate the TypeScript engine types in apps/web/src/lib/engine/gen from lbn-core (ts-rs).
# ts-rs writes 64-bit integers as `bigint`; JSON carries them as numbers (all values < 2^53), so they become `number`.
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf apps/web/src/lib/engine/gen
cargo test -q -p lbn-core --features ts export_bindings >/dev/null
find apps/web/src/lib/engine/gen -name '*.ts' -exec perl -pi -e 's/\bbigint\b/number/g' {} +   # perl: same on GNU and BSD
echo "generated $(find apps/web/src/lib/engine/gen -name '*.ts' | wc -l) files"
