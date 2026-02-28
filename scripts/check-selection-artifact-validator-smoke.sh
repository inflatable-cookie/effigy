#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VALIDATOR="$ROOT_DIR/scripts/validate-json-contract-selection-artifact.sh"

if [[ ! -x "$VALIDATOR" ]]; then
  echo "[error] validator script is missing or not executable: $VALIDATOR" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

valid_artifact="$TMP_DIR/selection-valid.json"
invalid_count_artifact="$TMP_DIR/selection-invalid-count.json"
invalid_mode_artifact="$TMP_DIR/selection-invalid-mode.json"
invalid_selected_item_artifact="$TMP_DIR/selection-invalid-selected-item.json"

cat >"$valid_artifact" <<'JSON'
{"selected":["effigy.tasks.v1"],"count":1,"changed_only_base":"HEAD","mode":"fast"}
JSON

cat >"$invalid_count_artifact" <<'JSON'
{"selected":["effigy.tasks.v1"],"count":2,"changed_only_base":"HEAD","mode":"fast"}
JSON

cat >"$invalid_mode_artifact" <<'JSON'
{"selected":["effigy.tasks.v1"],"count":1,"changed_only_base":"HEAD","mode":"unknown"}
JSON

cat >"$invalid_selected_item_artifact" <<'JSON'
{"selected":["effigy.tasks.v1",123],"count":2,"changed_only_base":"HEAD","mode":"fast"}
JSON

"$VALIDATOR" "$valid_artifact"

if "$VALIDATOR" "$invalid_count_artifact" >/dev/null 2>&1; then
  echo "[error] validator smoke check failed: invalid count fixture unexpectedly passed" >&2
  exit 1
fi

if "$VALIDATOR" "$invalid_mode_artifact" >/dev/null 2>&1; then
  echo "[error] validator smoke check failed: invalid mode fixture unexpectedly passed" >&2
  exit 1
fi

if "$VALIDATOR" "$invalid_selected_item_artifact" >/dev/null 2>&1; then
  echo "[error] validator smoke check failed: invalid selected-item fixture unexpectedly passed" >&2
  exit 1
fi

echo "[ok] validator smoke check passed (valid fixture accepted; count/mode/selected-item invalid fixtures rejected)"
