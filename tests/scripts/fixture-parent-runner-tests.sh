#!/usr/bin/env bash
set -euo pipefail

runner=${1:?test binary path is required}
if [ ! -f "$runner" ] || [ ! -x "$runner" ]; then
  printf 'fixture runner test binary is unavailable\n' >&2
  exit 2
fi

source "$(dirname "${BASH_SOURCE[0]}")/fixture-parent-test-driver.sh"

for test_name in \
  runner::state_command::tests::state_fixture_old_placement_refuses_all_four_execution_paths \
  runner::tests::catalog_membership_tests::nested_container_target_tests::root_qualified_nested_container_task_executes_in_declared_container \
  runner::tests::catalog_membership_tests::nested_container_target_tests::child_scope_nested_container_task_still_executes_in_declared_container \
  runner::tests::catalog_membership_tests::nested_container_target_tests::root_declared_host_task_stays_on_host \
  runner::tests::catalog_membership_tests::nested_container_target_tests::nested_task_without_own_target_falls_back_to_the_workspace_runtime \
  runner::tests::catalog_membership_tests::nested_container_target_tests::nested_container_task_without_any_declared_target_fails_before_execution \
  runner::tests::catalog_membership_tests::nested_container_target_tests::selector_plans_expose_the_resolved_runtime_target \
  runner::tests::catalog_membership_tests::nested_container_target_tests::builtin_nested_test_selection_executes_in_declared_container \
  runner::tests::catalog_membership_tests::nested_container_target_tests::builtin_test_plan_exposes_nested_declared_runtime \
  runner::tests::catalog_membership_tests::nested_container_target_tests::builtin_host_target_suites_stay_on_host_while_container_target_routes \
  runner::system_command::workspace::tests::bounded_linux_artifact_success_without_valid_output_refuses_without_rebinding \
  runner::state_command::tests::state_apply_executes_declared_artifact_layer_hook_with_context \
  runner::state_command::tests::state_apply_hook_receives_declared_state_secret \
  runner::state_command::tests::state_apply_can_skip_selected_layers \
  runner::state_command::tests::state_apply_reports_failed_hook_after_successful_artifact_stage \
  runner::state_command::tests::state_apply_accepts_inline_hook_task_in_composed_manifest \
  runner::state_command::tests::state_apply_rejects_unknown_skip_layers \
  runner::state_command::tests::state_capture_push_embeds_pushed_artifact_report \
  runner::state_command::tests::state_capture_executes_named_and_inline_tasks_with_context \
  runner::state_command::tests::state_layer_hook_accepts_compact_inline_task_run_in \
  runner::state_command::tests::state_capture_task_accepts_compact_inline_task_run_in \
  cli::version_dispatch::tests::build_version_payload_sets_schema_and_display \
  cli::help_dispatch::tests::build_help_payload_sets_schema_and_topic \
  cli::output::envelope::tests::build_json_envelope_error_sets_contract_shape \
  runner::gateway_command::tests::gateway_status_match_rejects_missing_binary_identity \
  runner::system_command::workspace::tests::linux_artifact_successful_build_without_fresh_output_is_refused \
  runner::system_command::workspace::tests::workspace_artifact_source_download_bypasses_discoverable_local_repo \
  runner::system_command::workspace::tests::workspace_effigy_local_install_identity_tracks_anchor_metadata \
  runner::cache_tests::builtin::cache_builtin_inspect_and_invalidate_paths_are_available \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_starts_when_requested \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_stages_db_seed_before_root_run \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_runs_standard_db_seed_task_before_start \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_auto_targets_single_database_bundle_seed_input \
  runner::bootstrap_command::tests::run_bootstrap_with_cwd_runs_multi_target_db_seed_task_for_bundle_databases \
  runner::tests::builtin_command_tests::execution_success_tests::run_manifest_task_builtin_test_executes_js_and_rust_suites_in_same_repo \
  runner::tests::builtin_command_tests::planning_and_selection_tests::vitest_detection_tests::run_manifest_task_builtin_test_skips_vitest_when_only_a_transitive_binary_exists \
  runner::tests::builtin_command_tests::scan_tests::dead_code::run_manifest_task_builtin_scan_dead_code_auto_refreshes_stale_index \
  runner::tests::runner_core_tests::deploy_tests::run_deploy_plan_blocks_required_release_without_tag_ref \
  runner::tests::builtin_command_tests::planning_and_selection_tests::package_scope_tests::run_manifest_task_builtin_test_plan_package_scope_narrows_cargo_command \
  runner::tests::builtin_command_tests::planning_and_selection_tests::package_scope_tests::run_manifest_task_builtin_test_plan_package_scope_covers_selection_forms \
  runner::tests::builtin_command_tests::planning_and_selection_tests::package_scope_tests::run_manifest_task_builtin_test_plan_package_scope_ignores_selectors_after_runner_separator \
  runner::tests::builtin_command_tests::planning_and_selection_tests::package_scope_tests::run_manifest_task_builtin_test_plan_package_scope_rejects_workspace_plus_package \
  runner::tests::builtin_command_tests::planning_and_selection_tests::package_scope_tests::run_manifest_task_builtin_test_package_scope_execution_omits_unrelated_failing_package \
  runner::tests::builtin_command_tests::planning_and_selection_tests::package_scope_tests::run_manifest_task_builtin_test_package_scope_execution_cargo_test_fallback \
  testing_tests::detect_test_runner_selects_all_cargo_workspace_members \
  runner::tests::run_array_tests::rhai_script_tests::run_manifest_task_run_array_supports_file_backed_rhai_steps \
  runner::tests::run_array_tests::rhai_script_tests::rhai_task_run_supports_prefixed_catalog_child_tasks_without_parent_lock_conflict \
  runner::tests::run_array_tests::rhai_script_tests::rhai_effigy_run_supports_prefixed_catalog_child_tasks_without_parent_lock_conflict \
  runner::tests::run_array_tests::rhai_script_tests::direct_rhai_task_run_supports_prefixed_catalog_child_tasks_without_parent_lock_conflict \
  runner::tests::catalog_membership_tests::membership_path_tests::effective_catalogs_include_named_member_mount_directories \
  runner::tests::catalog_membership_tests::membership_path_tests::effective_catalogs_include_inline_workspace_mount_directories \
  runner::tests::catalog_membership_tests::prefix_resolution_tests::run_manifest_task_relative_prefix_resolves_catalog_by_path \
  runner::tests::catalog_membership_tests::prefix_resolution_tests::run_manifest_task_relative_prefix_supports_multi_parent_traversal
do
  run_test "$test_name"
done
