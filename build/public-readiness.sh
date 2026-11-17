#!/usr/bin/env bash
# Checks that must pass before a PUBLIC release. Exits 1 and lists what is missing. It does not change anything.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
fail=0; bad() { echo "NOT READY: $*"; fail=1; }; ok() { echo "ok: $*"; }
grep -qi "development" keys/trusted.json && bad "keys/trusted.json still trusts the development sample-publisher key; replace it with the publisher's own key (docs/BUILDING.md)" || ok "no development key trusted"
[ -z "$(git status --porcelain)" ] && ok "worktree clean" || bad "uncommitted changes"
REL=$(ls -d dist/release/*/ 2>/dev/null | tail -1)
if [ -n "$REL" ]; then
  (cd "$REL" && sha256sum -c --quiet SHA256SUMS) && ok "release checksums ($REL)" || bad "release checksums"
  [ -f "$REL/SHA256SUMS.minisig" ] && ok "release checksums signed" || bad "SHA256SUMS is not signed"
  grep -q '"git_dirty": false' "$REL/BUILD-INFO.json" && ok "release built from a clean tree" || bad "release built from a dirty tree"
else bad "no release in dist/release (run build/release.sh)"; fi
grep -q "UNTESTED" runner/build-macos.sh && echo "note: macOS/Windows builds are untested (STATUS.md)"
exit $fail
