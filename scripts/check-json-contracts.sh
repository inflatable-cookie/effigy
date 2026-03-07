#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
INDEX_PATH="$ROOT_DIR/docs/contracts/json-schema-index.json"
MODE="full"
CHANGED_ONLY_BASE=""
PRINT_SELECTED_MODE="none"

source "$SCRIPT_DIR/lib/check-json-contracts/selection.sh"
source "$SCRIPT_DIR/lib/check-json-contracts/runtime.sh"
source "$SCRIPT_DIR/lib/check-json-contracts/checks.sh"

parse_json_contract_args "$@"
require_json_contract_prerequisites
init_json_contract_runtime
select_json_contract_rows
print_selected_json_contract_rows
run_selected_json_contract_checks
