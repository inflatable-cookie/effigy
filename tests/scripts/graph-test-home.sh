#!/bin/sh
# Graph index owns a host kernel gate. Tests must never create it in the user's HOME.
set -eu
graph_test_home=$(mktemp -d)
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$graph_test_home"
exec "$@"
