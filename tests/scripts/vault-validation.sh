#!/usr/bin/env bash
# Keep host state and ad-hoc temporary fixtures inside one owned directory.
set -euo pipefail
vault_validation_root="$(mktemp -d)"
trap 'rm -rf "$vault_validation_root"' EXIT
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$vault_validation_root"
export TMPDIR="$vault_validation_root"
"$@"
