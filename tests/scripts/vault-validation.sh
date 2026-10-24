#!/usr/bin/env bash
# Keep host state and ad-hoc temporary fixtures inside one owned directory.
set -euo pipefail
# An explicit template: macOS `mktemp -d` without one ignores TMPDIR.
vault_validation_root="$(mktemp -d "${TMPDIR:-/tmp}/effigy-vault-validation.XXXXXX")"
trap 'rm -rf "$vault_validation_root"' EXIT
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$vault_validation_root"
export TMPDIR="$vault_validation_root"
"$@"
