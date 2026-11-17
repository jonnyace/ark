#!/usr/bin/env bash
# CI only: make a throwaway minisign key, trust it in THIS workspace (keys/trusted.json, not committed), and print the
# secret key path. Test fixtures can then be signed without the publisher's real key, which never leaves its secret
# store. build/public-readiness.sh refuses any build that trusts a CI test key.
#   K=$(build/ci-test-key.sh) && LBN_SIGNING_KEY=$K build/ci.sh --e2e
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
D="${RUNNER_TEMP:-$(mktemp -d)}/lbn-ci-key"; mkdir -p "$D"; chmod 700 "$D"
cargo build -q --release --locked -p lbn-cli >&2
target/release/ark-pack key gen "$D/ci" --name "Ark publisher (CI test key, not for release)" >/dev/null
PK=$(tail -1 "$D/ci.pub")
printf '[\n  {"name": "Ark publisher (CI test key, not for release)", "public_key": "%s"}\n]\n' "$PK" > keys/trusted.json
cargo build -q --release --locked -p lbn-cli >&2    # rebuild so the CLI trusts the CI key
echo "$D/ci.key"
