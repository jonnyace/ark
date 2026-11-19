#!/usr/bin/env bash
# Every file that states Ark's release number must agree with the workspace Cargo.toml, the one source of truth.
#   build/check-versions.sh     prints the version, or each file that disagrees and exits 1
# Bump a release by editing all of them; this check (run first in build/ci.sh) catches the one you missed.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
V=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$V" ] || { echo "no version in Cargo.toml" >&2; exit 1; }
bad=0
check() { # file, the version found in it
  if [ "$2" != "$V" ]; then echo "$1 says '${2:-nothing}', Cargo.toml says $V" >&2; bad=1; fi
}
check apps/desktop/src-tauri/Cargo.toml "$(sed -n 's/^version = "\(.*\)"/\1/p' apps/desktop/src-tauri/Cargo.toml | head -1)"
for f in apps/web/package.json apps/desktop/package.json apps/desktop/src-tauri/tauri.conf.json; do
  check "$f" "$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$f" | head -1)"
done
check packaging/arch/PKGBUILD "$(sed -n 's/^_ver=//p' packaging/arch/PKGBUILD)"
[ "$bad" = 0 ] && echo "$V"
exit "$bad"
