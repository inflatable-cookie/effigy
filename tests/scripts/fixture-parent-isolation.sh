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

# The script owns TMPDIR for its own bounded cleanup. temp_workspace creates
# authenticated child roots under crates/ independently of TMPDIR; Cargo
# excludes that prefix while graph/scan walks still index fixture sources.
cargo nextest run \
  --config-file tests/config/gateway-nextest.toml \
  --user-config-file none \
  -p effigy \
  --lib \
  --filterset 'test(build_version_payload_sets_schema_and_display) | test(build_json_envelope_error_sets_contract_shape) | test(cache_builtin_inspect_and_invalidate_paths_are_available) | test(run_bootstrap_with_cwd_starts_when_requested) | test(run_bootstrap_with_cwd_stages_db_seed_before_root_run) | test(run_bootstrap_with_cwd_runs_standard_db_seed_task_before_start) | test(run_bootstrap_with_cwd_auto_targets_single_database_bundle_seed_input) | test(run_bootstrap_with_cwd_runs_multi_target_db_seed_task_for_bundle_databases) | test(run_manifest_task_builtin_test_executes_js_and_rust_suites_in_same_repo) | test(run_manifest_task_builtin_test_skips_vitest_when_only_a_transitive_binary_exists) | test(run_manifest_task_builtin_scan_dead_code_auto_refreshes_stale_index) | test(run_deploy_plan_blocks_required_release_without_tag_ref) | test(run_manifest_task_builtin_test_plan_package_scope_narrows_cargo_command) | test(run_manifest_task_builtin_test_plan_package_scope_covers_selection_forms) | test(run_manifest_task_builtin_test_plan_package_scope_ignores_selectors_after_runner_separator) | test(run_manifest_task_builtin_test_plan_package_scope_rejects_workspace_plus_package) | test(run_manifest_task_builtin_test_package_scope_execution_omits_unrelated_failing_package) | test(run_manifest_task_builtin_test_package_scope_execution_cargo_test_fallback) | test(detect_test_runner_selects_all_cargo_workspace_members) | test(run_manifest_task_run_array_supports_file_backed_rhai_steps) | test(rhai_task_run_supports_prefixed_catalog_child_tasks_without_parent_lock_conflict) | test(rhai_effigy_run_supports_prefixed_catalog_child_tasks_without_parent_lock_conflict) | test(direct_rhai_task_run_supports_prefixed_catalog_child_tasks_without_parent_lock_conflict) | test(effective_catalogs_include_named_member_mount_directories) | test(effective_catalogs_include_inline_workspace_mount_directories) | test(run_manifest_task_relative_prefix_resolves_catalog_by_path) | test(run_manifest_task_relative_prefix_supports_multi_parent_traversal)' \
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
