#!/usr/bin/env bash
# Checks that must pass before a PUBLIC release. Exits 1 and lists what is missing. It does not change anything.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
fail=0; bad() { echo "NOT READY: $*"; fail=1; }; ok() { echo "ok: $*"; }
if grep -qi "development" keys/trusted.json || grep -q "RWSTzXj3joIwkD19hylL8D8m5ZopADXT91Nyf6bNAX1QELa/wpYc51BH" keys/trusted.json; then
  bad "keys/trusted.json still trusts the retired development key; trust only the publisher's own key (docs/BUILDING.md)"
else ok "only the publisher key is trusted ($(grep -o '"name": "[^"]*"' keys/trusted.json | tr '\n' ' '))"; fi
[ -z "$(git status --porcelain)" ] && ok "worktree clean" || bad "uncommitted changes"
REL=$(ls -d dist/release/*/ 2>/dev/null | tail -1)
if [ -n "$REL" ]; then
  (cd "$REL" && sha256sum -c --quiet SHA256SUMS) && ok "release checksums ($REL)" || bad "release checksums"
  if [ -f "$REL/SHA256SUMS.minisig" ] && target/release/lastbook verify-file "$REL/SHA256SUMS" >/dev/null 2>&1; then ok "release checksums signed by a trusted key"
  else bad "SHA256SUMS is not signed by a key in keys/trusted.json"; fi
  [ -f "$REL/packs/catalog.json.minisig" ] && ok "catalog signed" || bad "packs/catalog.json is not signed"
  grep -q '"git_dirty": false' "$REL/BUILD-INFO.json" && ok "release built from a clean tree" || bad "release built from a dirty tree"
else bad "no release in dist/release (run build/release.sh)"; fi
grep -q "UNTESTED" runner/build-macos.sh && echo "note: macOS/Windows builds are untested (STATUS.md)"
exit $fail
