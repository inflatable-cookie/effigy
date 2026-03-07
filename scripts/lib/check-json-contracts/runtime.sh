init_json_contract_runtime() {
  SELECTED_ROWS_FILE="$(mktemp)"
  OLD_ACTIVE_FILE="$(mktemp)"
  FIXTURE_DIRS=()
  trap cleanup_json_contract_runtime EXIT
}

cleanup_json_contract_runtime() {
  rm -f "$SELECTED_ROWS_FILE" "$OLD_ACTIVE_FILE"
  for fixture_dir in "${FIXTURE_DIRS[@]:-}"; do
    rm -rf "$fixture_dir"
  done
}

create_json_contract_fixture() {
  local fixture_dir
  local task_body="$1"
  fixture_dir="$(mktemp -d)"
  FIXTURE_DIRS+=("$fixture_dir")
  printf '{}\n' >"$fixture_dir/package.json"
  cat >"$fixture_dir/effigy.toml" <<TOML
$task_body
TOML
  printf '%s\n' "$fixture_dir"
}

create_json_contract_success_fixture() {
  create_json_contract_fixture $'[tasks.build]\nrun = "printf build-ok"'
}

create_json_contract_failure_fixture() {
  create_json_contract_fixture $'[tasks.fail]\nrun = "sh -lc '\''printf fail-out; printf fail-err >&2; exit 9'\''"'
}

expand_json_contract_fixture_tokens() {
  local command="$1"
  local fixture_dir

  if [[ "$command" == *"<fixture_task_success>"* ]]; then
    fixture_dir="$(create_json_contract_success_fixture)"
    command="${command//<fixture_task_success>/$fixture_dir}"
  fi

  if [[ "$command" == *"<fixture_task_failure>"* ]]; then
    fixture_dir="$(create_json_contract_failure_fixture)"
    command="${command//<fixture_task_failure>/$fixture_dir}"
  fi

  printf '%s\n' "${command//<name>/test}"
}

run_effigy_json() {
  local command="$1"
  local expect_failure="$2"
  local args
  local payload
  local status

  command="$(expand_json_contract_fixture_tokens "$command")"
  if [[ "$command" != effigy* ]]; then
    echo "[error] index command must start with 'effigy': $command" >&2
    return 1
  fi

  args="${command#effigy }"
  set +e
  payload="$(cd "$ROOT_DIR" && cargo run --quiet --bin effigy -- $args)"
  status=$?
  set -e

  if [[ "$expect_failure" == "true" ]]; then
    if [[ "$status" -eq 0 ]]; then
      echo "[error] expected command to fail but it succeeded: $command" >&2
      return 1
    fi
  elif [[ "$status" -ne 0 ]]; then
    echo "[error] command failed unexpectedly (status=$status): $command" >&2
    return 1
  fi

  printf '%s' "$payload"
}
