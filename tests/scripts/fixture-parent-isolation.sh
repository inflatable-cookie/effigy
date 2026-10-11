#!/usr/bin/env bash
set -euo pipefail

: "${EFFIGY_HOST_RUN_PRIVATE_SERVER:?set this to the reviewed private Queue checkout}"

fixture_root=$(mktemp -d /tmp/effigy-parent-isolation.XXXXXX)
if [ ! -d "$fixture_root" ] || [ -L "$fixture_root" ]; then
  printf 'mktemp did not return an owned fixture directory\n' >&2
  exit 1
fi

cleanup() {
  local status=$?
  trap - EXIT
  if [ -d "$fixture_root" ] && [ ! -L "$fixture_root" ]; then
    rm -rf -- "$fixture_root" || status=$?
  else
    printf 'fixture root is missing or ambiguous; refusing cleanup\n' >&2
    status=1
  fi
  exit "$status"
}
trap cleanup EXIT

export TMPDIR="$fixture_root"

cargo nextest run \
  --config-file tests/config/gateway-nextest.toml \
  --user-config-file none \
  -p effigy \
  --lib \
  --filterset 'test(cache_builtin_inspect_and_invalidate_paths_are_available) | test(run_bootstrap_with_cwd_starts_when_requested) | test(run_bootstrap_with_cwd_stages_db_seed_before_root_run) | test(run_bootstrap_with_cwd_runs_standard_db_seed_task_before_start) | test(run_bootstrap_with_cwd_auto_targets_single_database_bundle_seed_input) | test(run_bootstrap_with_cwd_runs_multi_target_db_seed_task_for_bundle_databases) | test(run_manifest_task_builtin_test_executes_js_and_rust_suites_in_same_repo) | test(run_manifest_task_builtin_test_skips_vitest_when_only_a_transitive_binary_exists)' \
  --no-capture \
  --max-fail 1:immediate \
  --retries 0 \
  --no-tests fail \
  -j 1

EFFIGY_REQUIRE_HOST_RUN_PRIVATE_SERVER=1 \
  cargo nextest run \
    --config-file tests/config/gateway-nextest.toml \
    --user-config-file none \
    -p effigy \
    --test host_scheduler_integration \
    --run-ignored only \
    --filterset 'test(fixture_parent_runner_selector_succeeds_with_authenticated_parent_root) | test(forged_expired_and_outside_root_tokens_exit_77_and_never_queue) | test(admitted_container_selector_stays_on_host_and_executes_its_resolved_payload)' \
    --no-capture \
    --max-fail 1:immediate \
    --retries 0 \
    --no-tests fail \
    -j 1
