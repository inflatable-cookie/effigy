# Test ratings at `7a502af038b7fbc4889d6484ea78deb25c7daac5`

The tables contain 7,700 rows: 6,436 Rust and 1,264 Python tests. Forty parent-invoked helper and
child entrypoints plus one measurement utility are marked in `excluded`, leaving 7,659 scored
tests.

## Distribution (scores 0–10)

| Score | 0–6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|
| Tests | 0 | 1,639 | 2,434 | 2,797 | 789 |

Mean 8.36; median 8. There are no scored tests below the agreed floor of 7.

## Per-domain results

| Domain | Table rows | Scored tests | Mean | Score ≤ 6 |
|---|---:|---:|---:|---:|
| T1 containers | 1,305 | 1,304 | 7.87 | 0 |
| T2 gateway/host/managed | 623 | 617 | 8.37 | 0 |
| T3 kernel/custody | 858 | 835 | 8.60 | 0 |
| T4 CLI/config/engine/UI | 1,793 | 1,788 | 7.84 | 0 |
| T5 other Rust | 1,857 | 1,853 | 8.70 | 0 |
| T6 Python harness | 1,264 | 1,262 | 8.92 | 0 |

## Round 4c close

The final 39 below-floor rows were re-read against their assertions. Twenty-seven behavior tests
were re-rated at 7 or higher. Ten fixture or child entrypoints were removed from the scored tables
with their parent test cited as the covering route. One duplicate pending-progress test was folded
into the retained inherited-writer test, which checks nonzero lower bounds for both partial streams
and that reads cover retained bytes. The anchor spawn timing utility remains available as an
ignored measurement tool and is excluded from scoring; it has no behavioral threshold. Its table
row is removed from scored tests.

Presentation rows remain attached to the production path they verify: fake-clock reconciliation
and restart-policy progress frames, doctor color styling, and typed general-help rendering. Their
assertions pin the exact operator-visible output. Readiness, refusal, identity, route-retry,
workspace path, bootstrap compatibility, and performance rows retain their distinct outcomes.

The baseline census and previous-round history are recorded in the earlier table revision. This
summary reports the current ratings only; it is not a fresh test census or a whole-suite QA result.

## Milestone QA durations (2026-11-12)

Source main-suite JUnit run ID: `a1f2a9c5-8eeb-4cb9-8e4d-407e582448df` (started `2026-11-12T13:15:45Z`). Fault-selector JUnit IDs: custody `78bebeee-fba5-4f79-a4cc-9015e7b9f6cd`, faults `8a783230-024d-4eed-b7b7-921f094bf5b1`, matrix `215b9b8c-cfed-42cd-89b3-c7b78d21fac3`, gateway-budget `e08ef4b9-10cb-47bf-bbb3-c23305b163f1`, handoff `cc26440c-014a-4511-8e5d-3c0d417b863c`, helpers `3a747dd7-2ffb-4ed8-bf40-b5fbf7c614b1`, machine-facts `d6c50680-aec9-4db0-95a3-28863c79c982`, probe `2caf0ceb-f6b6-46c5-b657-2304ae766bff`, remote `a52cf768-6a84-4365-94f0-648d4f44987e`, and secrets-vault `20e29ae8-c77a-4237-b4ba-5017049fd7f2` (all 2026-11-12). Reports are retained under `target/nextest/`. Durations were matched to 5,060 quality rows; 1,197 report cases had no unique rated-row identity and remain blank. When a test appears in multiple reports, the table keeps its longest observed duration. `duration_ms` is measurement metadata and is excluded from `body_sha` and drift calculations.

The `effigy qa` invocation completed the main suite, kernel faults and matrix, then stopped in custody: 152 tests passed and `foreground_begins_beside_a_journal_written_throughout` failed its existing timing assertion at 1.248 seconds. Other workspaces started nextest runs during this collection window, so treat these timings as observed values under host contention, not isolated benchmarks. The remaining fault-injection selectors were run individually afterward; already-passed selectors were not repeated. This is a partial QA result, not a green milestone QA.

### Time in nextest test groups

| Group | Summed test duration | Tests | Max threads |
|---|---:|---:|---:|
| `cold-start` | 175,962 ms | 16 | 1 |
| `journal-matrix` | 14,690 ms | 1 | 1 |
| `kernel-fault-drivers` | 912,392 ms | 13 | 2 |

### Slowest 50 tests

| Duration | Selector / test |
|---:|---|
| 199,044 ms | `qa-kernel-faults` / `faults_sequence_every_point_and_observation` |
| 193,698 ms | `qa-kernel-faults` / `faults_environment_up_down_handover_with_concurrent_attach` |
| 160,271 ms | `qa-kernel-faults` / `faults_multi_child_test_every_point_and_observation` |
| 141,385 ms | `qa-kernel-faults` / `faults_detached_managed_run_with_environment_attachment` |
| 139,840 ms | `qa-kernel-matrix` / `matrix_supervisor_full_cross_product_releases_each_attachment_once` |
| 133,841 ms | `main` / `private_three_scopes_concurrent_resume_replacement_and_cli_refusals` |
| 112,972 ms | `qa-kernel-faults` / `faults_plain_task_every_point_and_observation` |
| 88,155 ms | `main` / `migration_failures_never_accept_stale_output_and_reuse_the_new_builder_on_retry` |
| 64,413 ms | `qa-kernel-custody` / `retained_interpolation_unsynced_binding_refuses_go_and_cannot_settle_as_plain_process` |
| 62,885 ms | `main` / `run_manifest_task_builtin_test_sweeps_a_leaked_suite_process` |
| 60,527 ms | `main` / `refusal_accounting_secret_partial_output_never_reaches_any_surface` |
| 60,302 ms | `qa-kernel-custody` / `prelaunch_interpolation_caller_drop_keeps_exact_descriptors_and_env_gate` |
| 60,258 ms | `main` / `cli_forward_handover_waits_for_owner_whose_stop_cascade_is_slow` |
| 53,118 ms | `main` / `managed_host_listener_retries_a_failed_republication_while_the_port_answers` |
| 45,616 ms | `main` / `cli_managed_headless_lifecycle_reaches_running_on_healthy_stack` |
| 44,323 ms | `main` / `shared_process_ambient_guard_regressions` |
| 44,047 ms | `main` / `detached_gateway_cleanup_covers_failure_exit_and_timeout_before_up_response` |
| 32,905 ms | `main` / `host_supervisor_restarts_after_a_drain_gap_with_inherited_session_flags` |
| 32,639 ms | `main` / `managed_host_dependent_replacement_preserves_routes_and_scope_teardown` |
| 28,987 ms | `main` / `managed_host_listeners_scoped_host_routes_follow_checkout_identity` |
| 25,582 ms | `qa-kernel-faults` / `faults_closed_predecessor_with_residue_then_begin` |
| 24,731 ms | `main` / `managed_host_listener_republishes_its_route_when_the_port_answers_again` |
| 24,696 ms | `qa-kernel-faults` / `faults_observe_with_guarded_subprocess_and_http_probes` |
| 23,179 ms | `main` / `detached_up_never_prints_a_secret_whose_marker_left_the_window` |
| 22,249 ms | `main` / `every_checkpoint_resumes_with_the_same_candidate` |
| 21,687 ms | `main` / `a_nested_effigy_run_started_by_user_code_never_inherits_the_session_access` |
| 21,671 ms | `main` / `workspace_reserved_every_guest_write_boundary_crash_before_after_preserves_authority` |
| 20,691 ms | `main` / `parked_integration_cleanup_survives_session_context_expiry_and_cancel` |
| 20,688 ms | `main` / `refusal_accounting_exact_eof_and_pending_states_coexist_in_one_snapshot` |
| 20,687 ms | `main` / `refusal_accounting_leader_exit_with_inherited_writer_keeps_partial_output_visible` |
| 20,587 ms | `main` / `parked_abort_uses_fresh_cleanup_after_context_expiry_and_cancel` |
| 20,538 ms | `main` / `creator_binding_expired_deadline_refuses_before_publication` |
| 20,375 ms | `main` / `detached_up_reconciles_only_the_current_declaration` |
| 19,789 ms | `qa-kernel-custody` / `fault_matrix_registration_spawn_and_socket_kill_enospc` |
| 19,508 ms | `main` / `a_nested_child_through_a_scrubbing_wrapper_stays_bounded` |
| 19,230 ms | `main` / `detached_up_budget_does_not_reconcile_a_withheld_dependents_old_generation` |
| 18,904 ms | `main` / `detached_up_second_activation_keeps_a_healthy_listener_past_the_budget` |
| 17,559 ms | `main` / `detached_up_withholds_a_listener_that_fails_its_current_readiness` |
| 17,200 ms | `main` / `managed_pane_feedback_stays_plain` |
| 17,142 ms | `main` / `detached_up_never_publishes_a_timed_out_listener_after_return` |
| 16,947 ms | `main` / `transfer_failures_preserve_the_prior_artifact` |
| 16,745 ms | `main` / `detached_up_reports_the_observed_open_port_when_the_activation_budget_expires` |
| 16,428 ms | `qa-kernel-faults` / `faults_objects_publish_promote_and_settle_preserve_complete_closures` |
| 16,284 ms | `main` / `managed_host_listener_probe_miss_does_not_restart_the_child` |
| 16,157 ms | `main` / `managed_matrix_detached_handover` |
| 16,007 ms | `main` / `cli_container_attached_session_handles_sigint_during_startup` |
| 15,405 ms | `main` / `container_hybrid_environment_switch_hands_the_whole_environment_over` |
| 14,990 ms | `main` / `managed_matrix_death_tui_hangup` |
| 14,690 ms | `main` / `run_supervision_matrix_recovery_stop_and_attachment_release_are_idempotent` |
