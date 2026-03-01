#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_PATH="${1:-$ROOT_DIR/target/release/effigy}"

if [[ ! -x "$BIN_PATH" ]]; then
  echo "[error] effigy binary is not executable: $BIN_PATH" >&2
  exit 1
fi

run_cmd() {
  local label="$1"
  shift
  echo "[check] $label"
  "$@" >/dev/null
  echo "[ok] $label"
}

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/effigy-release-smoke-XXXXXX")"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

cat >"$TMP_DIR/effigy.toml" <<'TOML'
[catalog]
alias = "farmyard"

[tasks]
noop = "echo noop"
TOML

cat >"$TMP_DIR/Cargo.toml" <<'TOML'
[package]
name = "release-smoke"
version = "0.1.0"
edition = "2021"
TOML

run_cmd "help" "$BIN_PATH" help
run_cmd "tasks" "$BIN_PATH" tasks --repo "$TMP_DIR"
run_cmd "prefixed builtin tasks" "$BIN_PATH" farmyard/tasks --repo "$TMP_DIR"
run_cmd "test plan" "$BIN_PATH" test --plan --repo "$TMP_DIR"
run_cmd "prefixed builtin test plan" "$BIN_PATH" farmyard/test --plan --repo "$TMP_DIR"
run_cmd "completion bash" "$BIN_PATH" completion bash
run_cmd "completion candidates" "$BIN_PATH" completion candidates --repo "$TMP_DIR"

echo "[ok] release smoke checks passed"
