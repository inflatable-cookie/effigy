is_heavy_json_contract_schema() {
  case "$1" in
    effigy.test.results.v1)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

assert_required_json_contract_keys() {
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

resolve_json_contract_schema_payload() {
  local expected_schema="$1"
  local payload="$2"
  local nested_error_schema
  local nested_result_schema

  RESOLVED_JSON_CONTRACT_SCHEMA="$(jq -r '.schema // empty' <<<"$payload")"
  RESOLVED_JSON_CONTRACT_VERSION="$(jq -r '.schema_version // empty' <<<"$payload")"
  RESOLVED_JSON_CONTRACT_PAYLOAD="$payload"

  if [[ "$RESOLVED_JSON_CONTRACT_SCHEMA" == "effigy.command.v1" && "$expected_schema" != "effigy.command.v1" ]]; then
    nested_result_schema="$(jq -r '.result.schema // empty' <<<"$payload")"
    nested_error_schema="$(jq -r '.error.details.schema // empty' <<<"$payload")"
    if [[ -n "$nested_result_schema" ]]; then
      RESOLVED_JSON_CONTRACT_PAYLOAD="$(jq '.result' <<<"$payload")"
      RESOLVED_JSON_CONTRACT_SCHEMA="$nested_result_schema"
      RESOLVED_JSON_CONTRACT_VERSION="$(jq -r '.result.schema_version // empty' <<<"$payload")"
    elif [[ -n "$nested_error_schema" ]]; then
      RESOLVED_JSON_CONTRACT_PAYLOAD="$(jq '.error.details' <<<"$payload")"
      RESOLVED_JSON_CONTRACT_SCHEMA="$nested_error_schema"
      RESOLVED_JSON_CONTRACT_VERSION="$(jq -r '.error.details.schema_version // empty' <<<"$payload")"
    fi
  fi
}

run_selected_json_contract_checks() {
  local failures=0
  local checks=0
  local row
  local schema
  local schema_version
  local command
  local expect_failure
  local payload
  local actual_schema
  local actual_version
  local payload_for_schema

  while IFS= read -r row; do
    [[ -z "$row" ]] && continue
    schema="$(jq -r '.schema' <<<"$row")"
    schema_version="$(jq -r '.schema_version' <<<"$row")"
    command="$(jq -r '.command' <<<"$row")"
    expect_failure="$(jq -r '.expect_failure // false' <<<"$row")"

    if [[ "$MODE" == "fast" ]] && is_heavy_json_contract_schema "$schema"; then
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

    resolve_json_contract_schema_payload "$schema" "$payload"
    actual_schema="$RESOLVED_JSON_CONTRACT_SCHEMA"
    actual_version="$RESOLVED_JSON_CONTRACT_VERSION"
    payload_for_schema="$RESOLVED_JSON_CONTRACT_PAYLOAD"

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

    if ! assert_required_json_contract_keys "$schema" "$payload_for_schema"; then
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
}
