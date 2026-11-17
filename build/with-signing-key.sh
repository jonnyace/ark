#!/usr/bin/env bash
# Run a signing command with the publisher's minisign secret key fetched from a secret store, never left on disk.
#   build/with-signing-key.sh <op://vault/item/field> <command...>
# The key is written to a private temporary file (mode 600) only while <command> runs, exported as LBN_SIGNING_KEY,
# then deleted. Maintainers without 1Password can instead set LBN_SIGNING_KEY=/path/to/key and run the command.
# Example: build/with-signing-key.sh "op://Eve/ew4z4v3vimlkzzmthqlxa5m4li/secret key" build/release.sh
set -euo pipefail
REF="$1"; shift
D=$(mktemp -d); chmod 700 "$D"
trap 'shred -u "$D/key" 2>/dev/null || rm -f "$D/key"; rmdir "$D"' EXIT
( umask 077; op read "$REF" > "$D/key" )
LBN_SIGNING_KEY="$D/key" "$@"
