#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTRACT_PATH="$ROOT_DIR/docs/contracts/json-selection-contract.json"
ARTIFACT_PATH="${1:-$ROOT_DIR/json-contracts-selected.json}"

if ! command -v jq >/dev/null 2>&1; then
  echo "[error] jq is required to validate selection artifacts." >&2
  exit 1
fi

if [[ ! -f "$CONTRACT_PATH" ]]; then
  echo "[error] contract not found: $CONTRACT_PATH" >&2
  exit 1
fi

if [[ ! -f "$ARTIFACT_PATH" ]]; then
  echo "[error] artifact not found: $ARTIFACT_PATH" >&2
  exit 1
fi

if ! jq -e . "$CONTRACT_PATH" >/dev/null 2>&1; then
  echo "[error] contract file is not valid JSON: $CONTRACT_PATH" >&2
  exit 1
fi

if ! jq -e . "$ARTIFACT_PATH" >/dev/null 2>&1; then
  echo "[error] artifact file is not valid JSON: $ARTIFACT_PATH" >&2
  exit 1
fi

required_keys_json="$(jq -c '.required' "$CONTRACT_PATH")"
mode_values_json="$(jq -c '.properties.mode.enum' "$CONTRACT_PATH")"

if ! jq -e \
  --argjson required "$required_keys_json" \
  --argjson mode_values "$mode_values_json" '
    . as $obj
    | (type == "object")
    and ($required | all(. as $key | ($obj | has($key))))
    and ($obj.selected | type == "array")
    and ($obj.selected | all(type == "string"))
    and ($obj.count | type == "number")
    and ($obj.count == ($obj.selected | length))
    and (($obj.changed_only_base == null) or ($obj.changed_only_base | type == "string"))
    and ($obj.mode | type == "string")
    and ($mode_values | index($obj.mode) != null)
  ' "$ARTIFACT_PATH" >/dev/null; then
  echo "[error] artifact does not satisfy selection payload contract: $ARTIFACT_PATH" >&2
  exit 1
fi

schema="$(jq -r '.schema' "$CONTRACT_PATH")"
schema_version="$(jq -r '.schema_version' "$CONTRACT_PATH")"
echo "[ok] selection artifact valid ($schema v$schema_version): $ARTIFACT_PATH"
