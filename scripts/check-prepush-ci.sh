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

run_step "json contracts (ci parity)" ./scripts/check-json-contracts-ci.sh
run_step "cli json mode tests" cargo test --test cli_output_tests cli_json_mode
run_step "library tests" cargo test --lib
run_step "cli output tests" cargo test --test cli_output_tests
run_step "docs links" ./scripts/check-doc-links.sh
run_step "docs json examples" ./scripts/check-doc-json-examples.sh

echo "[ok] pre-push ci checks passed"
