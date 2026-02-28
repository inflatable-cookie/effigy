#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INDEX_PATH="$ROOT_DIR/docs/contracts/json-schema-index.json"
MODE="full"
CHANGED_ONLY_BASE=""
PRINT_SELECTED_MODE="none"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fast)
      MODE="fast"
      shift
      ;;
    --full)
      MODE="full"
      shift
      ;;
    --changed-only)
      if [[ $# -lt 2 ]]; then
        echo "[error] --changed-only requires a git base ref (for example: main or HEAD~1)" >&2
        exit 1
      fi
      CHANGED_ONLY_BASE="$2"
      shift 2
      ;;
    --print-selected)
      PRINT_SELECTED_MODE="text"
      shift
      ;;
    --print-selected=text)
      PRINT_SELECTED_MODE="text"
      shift
      ;;
    --print-selected=json)
      PRINT_SELECTED_MODE="json"
      shift
      ;;
    *)
      echo "[error] unknown option: $1 (supported: --fast, --full, --changed-only <base-ref>, --print-selected, --print-selected=text, --print-selected=json)" >&2
      exit 1
      ;;
  esac
done

if ! command -v jq >/dev/null 2>&1; then
  echo "[error] jq is required to run JSON contract checks." >&2
  exit 1
fi

if [[ ! -f "$INDEX_PATH" ]]; then
  echo "[error] schema index not found: $INDEX_PATH" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "[error] cargo is required to run effigy for contract checks." >&2
  exit 1
fi

# Validate index shape first.
jq -e '.version and (.schemas | type == "array")' "$INDEX_PATH" >/dev/null

SELECTED_ROWS_FILE="$(mktemp)"
OLD_ACTIVE_FILE="$(mktemp)"
declare -a FIXTURE_DIRS=()
cleanup() {
  rm -f "$SELECTED_ROWS_FILE" "$OLD_ACTIVE_FILE"
  for fixture_dir in "${FIXTURE_DIRS[@]:-}"; do
    rm -rf "$fixture_dir"
  done
}
trap cleanup EXIT

if [[ -n "$CHANGED_ONLY_BASE" ]]; then
  if ! git -C "$ROOT_DIR" rev-parse --verify "$CHANGED_ONLY_BASE^{commit}" >/dev/null 2>&1; then
    echo "[error] invalid git base ref for --changed-only: $CHANGED_ONLY_BASE" >&2
    exit 1
  fi

  old_index_content="$(git -C "$ROOT_DIR" show "$CHANGED_ONLY_BASE:docs/contracts/json-schema-index.json" 2>/dev/null || true)"
  if [[ -z "$old_index_content" ]]; then
    jq -cS '.schemas[] | select(.status == "active")' "$INDEX_PATH" >"$SELECTED_ROWS_FILE"
  else
    if ! jq -cS '.schemas[]? | select(.status == "active")' <<<"$old_index_content" >"$OLD_ACTIVE_FILE" 2>/dev/null; then
      echo "[warn] base ref index is invalid JSON; treating all active schemas as changed" >&2
      jq -cS '.schemas[] | select(.status == "active")' "$INDEX_PATH" >"$SELECTED_ROWS_FILE"
    else
      jq -nrcS \
        --slurpfile current "$INDEX_PATH" \
        --slurpfile old_rows "$OLD_ACTIVE_FILE" '
          ($current[0].schemas | map(select(.status == "active"))) as $active_current
          | (reduce $old_rows[] as $row ({}; .[$row.schema] = $row)) as $old_map
          | $active_current[]
          | select((.schema as $schema | ($old_map[$schema] // null)) != .)
        ' >"$SELECTED_ROWS_FILE"
    fi
  fi
else
  jq -cS '.schemas[] | select(.status == "active")' "$INDEX_PATH" >"$SELECTED_ROWS_FILE"
fi

if [[ "$PRINT_SELECTED_MODE" == "text" ]]; then
  selected_count="$(grep -cve '^[[:space:]]*$' "$SELECTED_ROWS_FILE" || true)"
  if [[ "$selected_count" -eq 0 ]]; then
    if [[ -n "$CHANGED_ONLY_BASE" ]]; then
      echo "[selected] none (no changed active schemas vs $CHANGED_ONLY_BASE)"
    else
      echo "[selected] none"
    fi
  else
    while IFS= read -r selected_schema; do
      [[ -z "$selected_schema" ]] && continue
      echo "[selected] $selected_schema"
    done < <(jq -r '.schema' "$SELECTED_ROWS_FILE")
  fi
fi

if [[ "$PRINT_SELECTED_MODE" == "json" ]]; then
  selected_json="$(jq -cs '[.[] | .schema]' "$SELECTED_ROWS_FILE")"
  if [[ -z "$selected_json" ]]; then
    selected_json="[]"
  fi

  selection_payload="$(
    jq -nrc \
    --argjson selected "$selected_json" \
    --arg mode "$MODE" \
    --arg base "$CHANGED_ONLY_BASE" \
    '{
      selected: $selected,
      count: ($selected | length),
      changed_only_base: (if $base == "" then null else $base end),
      mode: $mode
    }'
  )"

  if ! jq -e '
      (type == "object")
      and (has("selected") and (.selected | type == "array"))
      and (has("count") and (.count | type == "number"))
      and (has("changed_only_base") and ((.changed_only_base == null) or (.changed_only_base | type == "string")))
      and (has("mode") and (.mode == "fast" or .mode == "full"))
      and (.count == (.selected | length))
      and ((.selected | all(type == "string")))
    ' >/dev/null <<<"$selection_payload"; then
    echo "[error] selection payload contract assertion failed" >&2
    exit 1
  fi

  echo "$selection_payload"
fi

run_effigy_json() {
  local command="$1"
  local expect_failure="$2"

  if [[ "$command" == *"<fixture_task_success>"* ]]; then
    local fixture_success
    fixture_success="$(mktemp -d)"
    FIXTURE_DIRS+=("$fixture_success")
    printf '{}\n' > "$fixture_success/package.json"
    cat > "$fixture_success/effigy.toml" <<'TOML'
[tasks.build]
run = "printf build-ok"
TOML
    command="${command//<fixture_task_success>/$fixture_success}"
  fi

  if [[ "$command" == *"<fixture_task_failure>"* ]]; then
    local fixture_failure
    fixture_failure="$(mktemp -d)"
    FIXTURE_DIRS+=("$fixture_failure")
    printf '{}\n' > "$fixture_failure/package.json"
    cat > "$fixture_failure/effigy.toml" <<'TOML'
[tasks.fail]
run = "sh -lc 'printf fail-out; printf fail-err >&2; exit 9'"
TOML
    command="${command//<fixture_task_failure>/$fixture_failure}"
  fi

  # Replace index placeholders with deterministic fixture args.
  command="${command//<name>/test}"

  if [[ "$command" != effigy* ]]; then
    echo "[error] index command must start with 'effigy': $command" >&2
    return 1
  fi

  local args="${command#effigy }"
  local payload
  local status
  set +e
  payload="$(cd "$ROOT_DIR" && cargo run --quiet --bin effigy -- $args)"
  status=$?
  set -e

  if [[ "$expect_failure" == "true" ]]; then
    if [[ "$status" -eq 0 ]]; then
      echo "[error] expected command to fail but it succeeded: $command" >&2
      return 1
    fi
  else
    if [[ "$status" -ne 0 ]]; then
      echo "[error] command failed unexpectedly (status=$status): $command" >&2
      return 1
    fi
  fi
  printf '%s' "$payload"
}

is_heavy_schema() {
  local schema="$1"
  case "$schema" in
    effigy.test.results.v1)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

assert_required_keys() {
  local schema="$1"
  local json_payload="$2"
  case "$schema" in
    effigy.command.v1)
      jq -e 'has("schema") and has("schema_version") and has("ok") and has("command") and (.command | type == "object") and (.command | has("kind")) and (.command | has("name")) and has("result") and has("error")' <<<"$json_payload" >/dev/null
      ;;
    *)
      echo "[error] unknown schema in checker: $schema" >&2
      return 1
      ;;
  esac
}

failures=0
checks=0

while IFS= read -r row; do
  [[ -z "$row" ]] && continue
  schema="$(jq -r '.schema' <<<"$row")"
  schema_version="$(jq -r '.schema_version' <<<"$row")"
  command="$(jq -r '.command' <<<"$row")"
  expect_failure="$(jq -r '.expect_failure // false' <<<"$row")"

  if [[ "$MODE" == "fast" ]] && is_heavy_schema "$schema"; then
    echo "[skip] $schema :: skipped in --fast mode"
    continue
  fi

  echo "[check] $schema v$schema_version :: $command"
  checks=$((checks + 1))
  if ! payload="$(run_effigy_json "$command" "$expect_failure")"; then
    echo "  [fail] command execution failed" >&2
    failures=$((failures + 1))
    continue
  fi

  if ! jq -e . >/dev/null 2>&1 <<<"$payload"; then
    echo "  [fail] output is not valid JSON" >&2
    failures=$((failures + 1))
    continue
  fi

  actual_schema="$(jq -r '.schema // empty' <<<"$payload")"
  actual_version="$(jq -r '.schema_version // empty' <<<"$payload")"
  payload_for_schema="$payload"
  if [[ "$actual_schema" == "effigy.command.v1" && "$schema" != "effigy.command.v1" ]]; then
    nested_result_schema="$(jq -r '.result.schema // empty' <<<"$payload")"
    nested_error_schema="$(jq -r '.error.details.schema // empty' <<<"$payload")"
    if [[ -n "$nested_result_schema" ]]; then
      payload_for_schema="$(jq '.result' <<<"$payload")"
      actual_schema="$nested_result_schema"
      actual_version="$(jq -r '.result.schema_version // empty' <<<"$payload")"
    elif [[ -n "$nested_error_schema" ]]; then
      payload_for_schema="$(jq '.error.details' <<<"$payload")"
      actual_schema="$nested_error_schema"
      actual_version="$(jq -r '.error.details.schema_version // empty' <<<"$payload")"
    fi
  fi

  if [[ "$actual_schema" != "$schema" ]]; then
    echo "  [fail] schema mismatch: expected=$schema actual=${actual_schema:-<missing>}" >&2
    failures=$((failures + 1))
    continue
  fi

  if [[ "$actual_version" != "$schema_version" ]]; then
    echo "  [fail] schema_version mismatch: expected=$schema_version actual=${actual_version:-<missing>}" >&2
    failures=$((failures + 1))
    continue
  fi

  if ! assert_required_keys "$schema" "$payload_for_schema"; then
    echo "  [fail] required keys missing for $schema" >&2
    failures=$((failures + 1))
    continue
  fi

  echo "  [ok] schema and required keys validated"
done <"$SELECTED_ROWS_FILE"

if [[ "$failures" -gt 0 ]]; then
  echo "[error] JSON contract checks failed: $failures failure(s)" >&2
  exit 1
fi

if [[ "$checks" -eq 0 ]]; then
  if [[ -n "$CHANGED_ONLY_BASE" ]]; then
    echo "[ok] JSON contract checks passed (no changed active schema entries vs $CHANGED_ONLY_BASE)"
  else
    echo "[ok] JSON contract checks passed (no applicable schema entries to validate)"
  fi
  exit 0
fi

echo "[ok] JSON contract checks passed"
