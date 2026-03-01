#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

run_step() {
  local label="$1"
  shift
  echo "[check] $label"
  "$@"
  echo "[ok] $label"
}

cd "$ROOT_DIR"

run_step "format check" cargo fmt --all -- --check
run_step "full test suite" cargo test
run_step "quality gates (ci mode)" ./scripts/check-quality-gates.sh --all --ci
run_step "release binary build" cargo build --release --bin effigy
run_step "release binary smoke" ./scripts/check-release-smoke.sh ./target/release/effigy

echo "[ok] release gates passed"
