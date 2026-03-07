parse_json_contract_args() {
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
}

require_json_contract_prerequisites() {
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

  jq -e '.version and (.schemas | type == "array")' "$INDEX_PATH" >/dev/null
}

select_all_active_json_contract_rows() {
  jq -cS '.schemas[] | select(.status == "active")' "$INDEX_PATH" >"$SELECTED_ROWS_FILE"
}

select_changed_json_contract_rows() {
  if ! git -C "$ROOT_DIR" rev-parse --verify "$CHANGED_ONLY_BASE^{commit}" >/dev/null 2>&1; then
    echo "[error] invalid git base ref for --changed-only: $CHANGED_ONLY_BASE" >&2
    exit 1
  fi

  old_index_content="$(git -C "$ROOT_DIR" show "$CHANGED_ONLY_BASE:docs/contracts/json-schema-index.json" 2>/dev/null || true)"
  if [[ -z "$old_index_content" ]]; then
    select_all_active_json_contract_rows
    return
  fi

  if ! jq -cS '.schemas[]? | select(.status == "active")' <<<"$old_index_content" >"$OLD_ACTIVE_FILE" 2>/dev/null; then
    echo "[warn] base ref index is invalid JSON; treating all active schemas as changed" >&2
    select_all_active_json_contract_rows
    return
  fi

  jq -nrcS \
    --slurpfile current "$INDEX_PATH" \
    --slurpfile old_rows "$OLD_ACTIVE_FILE" '
      ($current[0].schemas | map(select(.status == "active"))) as $active_current
      | (reduce $old_rows[] as $row ({}; .[$row.schema] = $row)) as $old_map
      | $active_current[]
      | select((.schema as $schema | ($old_map[$schema] // null)) != .)
    ' >"$SELECTED_ROWS_FILE"
}

select_json_contract_rows() {
  if [[ -n "$CHANGED_ONLY_BASE" ]]; then
    select_changed_json_contract_rows
  else
    select_all_active_json_contract_rows
  fi
}

print_selected_json_contract_rows_text() {
  local selected_count
  selected_count="$(grep -cve '^[[:space:]]*$' "$SELECTED_ROWS_FILE" || true)"
  if [[ "$selected_count" -eq 0 ]]; then
    if [[ -n "$CHANGED_ONLY_BASE" ]]; then
      echo "[selected] none (no changed active schemas vs $CHANGED_ONLY_BASE)"
    else
      echo "[selected] none"
    fi
    return
  fi

  while IFS= read -r selected_schema; do
    [[ -z "$selected_schema" ]] && continue
    echo "[selected] $selected_schema"
  done < <(jq -r '.schema' "$SELECTED_ROWS_FILE")
}

print_selected_json_contract_rows_json() {
  local selected_json
  local selection_payload
  selected_json="$(jq -cs '[.[] | .schema]' "$SELECTED_ROWS_FILE")"
  if [[ -z "$selected_json" ]]; then
    selected_json="[]"
  fi

  selection_payload="$(
    jq -nrc \
      --argjson selected "$selected_json" \
      --arg mode "$MODE" \
      --arg base "$CHANGED_ONLY_BASE" '
        {
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
      and (.selected | all(type == "string"))
    ' >/dev/null <<<"$selection_payload"; then
    echo "[error] selection payload contract assertion failed" >&2
    exit 1
  fi

  echo "$selection_payload"
}

print_selected_json_contract_rows() {
  case "$PRINT_SELECTED_MODE" in
    text)
      print_selected_json_contract_rows_text
      ;;
    json)
      print_selected_json_contract_rows_json
      ;;
  esac
}
