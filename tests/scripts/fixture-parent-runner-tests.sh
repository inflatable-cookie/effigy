#!/usr/bin/env bash
set -euo pipefail

runner=${1:?test binary path is required}
if [ ! -f "$runner" ] || [ ! -x "$runner" ]; then
  printf 'fixture runner test binary is unavailable\n' >&2
  exit 2
fi

run_test() {
  local test_name=$1
  local status=0
  local deadline

  "$runner" "$test_name" --exact --nocapture --test-threads=1 &
  local test_pid=$!
  deadline=$((SECONDS + 180))
  while kill -0 "$test_pid" 2>/dev/null; do
    if (( SECONDS >= deadline )); then
      printf 'fixture test timed out; terminating owned PID %s\n' "$test_pid" >&2
      kill -TERM "$test_pid" 2>/dev/null || true
      for _ in {1..20}; do
        kill -0 "$test_pid" 2>/dev/null || break
        sleep 0.1
      done
      kill -KILL "$test_pid" 2>/dev/null || true
      wait "$test_pid" || true
      return 124
    fi
    sleep 1
  done
  wait "$test_pid" || status=$?
  return "$status"
}

for test_name in \
  runner::cache_tests::builtin::cache_builtin_inspect_and_invalidate_paths_are_available \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_starts_when_requested \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_stages_db_seed_before_root_run \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_runs_standard_db_seed_task_before_start \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_auto_targets_single_database_bundle_seed_input \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_runs_multi_target_db_seed_task_for_bundle_databases \
  runner::tests::builtin_command_tests::execution_success_tests::run_manifest_task_builtin_test_executes_js_and_rust_suites_in_same_repo \
  runner::tests::builtin_command_tests::planning_and_selection_tests::vitest_detection_tests::run_manifest_task_builtin_test_skips_vitest_when_only_a_transitive_binary_exists \
  runner::tests::builtin_command_tests::scan_tests::dead_code::run_manifest_task_builtin_scan_dead_code_auto_refreshes_stale_index \
  runner::tests::runner_core_tests::deploy_tests::run_deploy_plan_blocks_required_release_without_tag_ref
do
  run_test "$test_name"
done
