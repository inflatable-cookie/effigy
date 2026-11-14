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

## QA duration history and current collection (2026-11-12–13)

Source main-suite JUnit run ID: `a1f2a9c5-8eeb-4cb9-8e4d-407e582448df` (started `2026-11-12T13:15:45Z`). Fault-selector JUnit IDs: custody `78bebeee-fba5-4f79-a4cc-9015e7b9f6cd`, faults `8a783230-024d-4eed-b7b7-921f094bf5b1`, matrix `215b9b8c-cfed-42cd-89b3-c7b78d21fac3`, gateway-budget `e08ef4b9-10cb-47bf-bbb3-c23305b163f1`, handoff `cc26440c-014a-4511-8e5d-3c0d417b863c`, helpers `3a747dd7-2ffb-4ed8-bf40-b5fbf7c614b1`, machine-facts `d6c50680-aec9-4db0-95a3-28863c79c982`, probe `2caf0ceb-f6b6-46c5-b657-2304ae766bff`, remote `a52cf768-6a84-4365-94f0-648d4f44987e`, and secrets-vault `20e29ae8-c77a-4237-b4ba-5017049fd7f2` (all 2026-11-12). Reports are retained under `target/nextest/`. Durations were matched to 5,060 quality rows; 1,197 report cases had no unique rated-row identity and remain blank. When a test appears in multiple reports, the table keeps its longest observed duration. `duration_ms` is measurement metadata and is excluded from `body_sha` and drift calculations.

The `effigy qa` invocation completed the main suite, kernel faults and matrix, then stopped in custody: 152 tests passed and `foreground_begins_beside_a_journal_written_throughout` failed its existing timing assertion at 1.248 seconds. Other workspaces started nextest runs during this collection window, so treat these timings as observed values under host contention, not isolated benchmarks. The remaining fault-injection selectors were run individually afterward; already-passed selectors were not repeated. This is a partial QA result, not a green milestone QA.

### Baseline time in nextest test groups (2026-11-12)

| Group | Summed test duration | Tests | Max threads |
|---|---:|---:|---:|
| `cold-start` | 175,962 ms | 16 | 1 |
| `journal-matrix` | 14,690 ms | 1 | 1 |
| `kernel-fault-drivers` | 912,392 ms | 13 | 2 |

### Historical main-suite slowest 50 baseline (f1b905bb, 2026-11-13)

| Duration | Selector / test |
|---:|---|
| 144,864 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::migration_failures_never_accept_stale_output_and_reuse_the_new_builder_on_retry` |
| 95,317 ms | `main` / `effigy-release::run::tests::every_checkpoint_resumes_with_the_same_candidate` |
| 80,537 ms | `main` / `effigy::contract_test_support::shared_process_ambient_guard_regressions` |
| 75,742 ms | `main` / `effigy::gateway_private::managed_host_dependent_replacement_preserves_routes_and_scope_teardown` |
| 70,153 ms | `main` / `effigy::cli_output_tests::command_behavior_tests::cli_forward_handover_waits_for_owner_whose_stop_cascade_is_slow` |
| 68,998 ms | `main` / `effigy::gateway_private::managed_host_listeners_scoped_host_routes_follow_checkout_identity` |
| 66,316 ms | `main` / `effigy::runner::tests::builtin_command_tests::execution_failure_tests::run_manifest_task_builtin_test_sweeps_a_leaked_suite_process` |
| 64,769 ms | `main` / `effigy::cli_output_tests::command_behavior_tests::cli_managed_headless_lifecycle_reaches_running_on_healthy_stack` |
| 56,923 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_tui_handover` |
| 55,721 ms | `main` / `effigy::runner::workspace_command::provisioning::reserved_guest::tests::workspace_reserved_every_guest_write_boundary_crash_before_after_preserves_authority` |
| 55,003 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::compiler_writes_through_the_read_only_source_bind_are_denied` |
| 52,900 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_tui_interrupt` |
| 52,028 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_death_tui_hangup` |
| 51,796 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_death_tui_terminal_close` |
| 49,188 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_tui_request` |
| 49,044 ms | `main` / `effigy::managed_pane_feedback::a_nested_child_through_a_scrubbing_wrapper_stays_bounded` |
| 48,451 ms | `main` / `effigy::gateway_private::container_headless_managed_lifecycle_publishes_routes_in_scoped_data_from_clone` |
| 47,752 ms | `main` / `effigy::gateway_private::container_scoped_clone_first_listener_up_and_switch_round_trip_hands_over` |
| 47,603 ms | `main` / `effigy::runner::container_command::data_custody_tests::every_data_command_route_keeps_the_password_out_of_argv_and_reports` |
| 46,860 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_death_text_terminate` |
| 46,593 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::a_collision_in_the_managed_namespace_is_never_treated_as_empty` |
| 46,260 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_tui_client_kill` |
| 44,670 ms | `main` / `effigy::cli_output_tests::partial_startup_tests::private_three_scopes_concurrent_resume_replacement_and_cli_refusals` |
| 44,112 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_detached_handover` |
| 43,822 ms | `main` / `effigy-release::run::tests::an_unknown_task_publish_with_a_check_reconciles_before_any_retry` |
| 42,398 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::a_writer_still_holding_the_cargo_tree_refuses_as_unknown_closure` |
| 41,073 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::recovery_cannot_publish_failed_wrong_target_or_unchanged_old_output` |
| 39,756 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_death_text_hangup` |
| 39,440 ms | `main` / `effigy::runner::tests::run_array_tests::container_up_exposure_tests::detached_up_never_prints_a_secret_whose_marker_left_the_window` |
| 39,396 ms | `main` / `effigy::gateway_private::linked_base_domain_alias_with_dns_route` |
| 39,351 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::handoff_migrates_the_authentic_legacy_namespace_without_manual_commands` |
| 38,490 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_death_tui_kill_before_active` |
| 38,149 ms | `main` / `effigy::cli_output_tests::release_platform_tests::effigy_release_run_certifies_and_then_publishes_through_the_fake_dispatch` |
| 38,041 ms | `main` / `effigy::gateway_private::container_hybrid_environment_switch_hands_the_whole_environment_over` |
| 37,938 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_death_tui_terminate` |
| 37,844 ms | `main` / `effigy::gateway_private::adapterless_listener::managed_host_listener_retries_a_failed_republication_while_the_port_answers` |
| 36,946 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::failed_and_interrupted_changed_input_builds_retain_admission_and_retry` |
| 36,104 ms | `main` / `effigy::gateway_private::fresh_linked_hybrid_dev_prepares_scope_before_policy` |
| 35,790 ms | `main` / `effigy::runner::host_process::tests::host_supervisor_restarts_after_a_drain_gap_with_inherited_session_flags` |
| 35,750 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::colima_native_power_is_recovered_or_refused_before_claim` |
| 35,188 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_detached_request` |
| 34,583 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_text_handover` |
| 34,364 ms | `main` / `effigy::runner::tests::run_array_tests::container_up_exposure_tests::detached_up_second_activation_keeps_a_healthy_listener_past_the_budget` |
| 34,304 ms | `main` / `effigy-release::run::tests::a_partial_tag_fan_out_resumes_with_the_remaining_identity` |
| 34,144 ms | `main` / `effigy::gateway_private::adapterless_listener::managed_host_listener_republishes_its_route_when_the_port_answers_again` |
| 33,731 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::extra_declared_service_refuses_and_legacy_receipts_stay_inert` |
| 33,599 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::transfer_failures_preserve_the_prior_artifact` |
| 32,880 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::managed_namespace::root_labelled` |
| 31,870 ms | `main` / `effigy::gateway_private::managed_matrix::managed_matrix_text_interrupt` |
| 31,728 ms | `main` / `effigy::runner::workspace_command::tests::builder_recovery::an_interrupted_incremental_stage_is_completed_by_the_next_run` |

### Speed-round 4 scheduling A/B (2026-11-13)

The same 73-test filter union was run serially (A1) and with the default
four-thread cold-start cap (B1). This selection matches 73 tests across 88
binaries; it is broader than the 16 mapped `cold-start` rows above, so these
figures are selector-level evidence, not a replacement group total.

| Run | Cold-start cap | Result | Nextest wall | Selector wall | Load average at start → end |
|---|---:|---|---:|---:|---|
| A1 | 1 | 73/73 passed | 1,397.541 s | 2,067.64 s | 79.49/78.61/76.68 → 54.99/68.93/74.61 |
| B1 | 4 | 73/73 passed | 337.997 s | 354.40 s | 61.64/67.17/73.41 → 79.64/84.91/81.16 |

The two clean runs were back to back and had no prohibited native-smoke or
fault-injection run active. A1's selector wall includes a 9m37s first-use test
profile compile and Oxide queueing; B1's includes a 3.47s incremental compile.
An attempted second 73-test pair overlapped `linux_native_smoke.py`; A2
completed 73 tests but is not clean evidence, and B2 was interrupted at 37/73
to stop the overlap. Neither is counted. A second clean pair for the exact
16-test `cold-start` group is recorded below. The required current-main JUnit
refresh is recorded below. The complete current main-suite duration collection is recorded below; it is
not a green suite.

The exact 16-test `cold-start` selector was run serially (A3, cap 1) and with
the committed four-thread cap (B3). Both runs passed all 16 tests. Each test
uses a private fixture root and listener, so this group does not share mutable
test resources; assertions, deadlines and fixture behavior are unchanged.

| Run | Cold-start cap | Result | Nextest wall | Selector wall | Load average at start → end |
|---|---:|---|---:|---:|---|
| A3 | 1 | 16/16 passed | 275.772 s | 292.85 s | 62.32/72.49/67.96 → 116.49/93.34/77.71 |
| B3 | 4 | 16/16 passed | 84.070 s | 110.36 s | 99.25/92.00/77.80 → 69.89/80.96/75.00 |

The pair had no native-smoke, fault-injection or milestone-QA run active.
Load was high in both runs, so these are observed A/B values rather than an
isolated host benchmark.

### Current main-suite duration collection and closeout validation (2026-11-14)

The T1–T5 `duration_ms` values below were refreshed from complete JUnit report
`main.xml`, source run `dada7636-027d-47d5-bd03-9f44b00b5c65`, collected at
`2026-11-14T01:11:41.861Z`: 6,278 cases, 6,275 passed, 3 failed and 31
skipped. Nextest wall was 363.851 s; summed case duration was 5,361,707 ms.
The slowest-50 table above is the historical list from the prior report
`main.xml`, source run `f1b905bb-4fb0-4895-b054-0d31028cf26a`, collected on
2026-11-13. It is not sorted from run `dada7636`; the quality selector refreshes
CSV duration rows but does not rewrite this Markdown table. Read the current
per-test durations from T1–T5 and the table above as historical baseline data.
`effigy qa:quality:tests -- --durations target/nextest/default/main.xml --top 50`
updated 4,819 quality rows; 1,459 cases had no unique table identity. The
main `cold-start` group accumulated 267,364 ms across 16 tests; the default
configuration in `.config/nextest.toml` caps that group at four. The quality
table command also reads gateway-specific nextest configuration, where the
same group name has a one-thread cap; its printed group limit is therefore not
the default-suite setting. `journal-matrix` accumulated 16,674 ms at one
thread. Load at suite start was 57.42/55.51/56.74. Other Effigy tests,
including fault selectors, were active; no `linux_native_smoke` was running.
These are observations under host contention, not isolated performance
measurements, and this full run was not green.

The failures were all reproduced or classified after the full command:

- `builtin_completion_json_contract_has_versioned_shape` failed in the full
  suite and focused run `80ee02b9-b9ef-4943-9a93-a87648125500` (1/1). The
  current command inventory differs from the older command list embedded in
  the guide's JSON example. The inventory change was merged in PR #1001 (task
  969); this baseline mismatch is outside this PR.
- `managed_headless_status_logs_and_stop_control_live_session` aborted in the
  full suite and focused run `06f20fb8-3526-48a3-8d13-39c765d961c8` with the
  same `torn gate body` refusal at `crates/effigy-kernel/src/gate.rs:875`, then
  a 30 s fixture-helper wait timeout. The gate's fail-closed parse behavior is
  from PR #334; the unchanged status test originated in PR #366. It reproduced
  while other Effigy selectors were active, and no code in this PR touches the
  gate or this test.
- `container_scoped_clone_first_listener_up_and_switch_round_trip_hands_over`
  failed with a TLS `AccessDenied` alert in this full run (23.325 s). It also
  failed in the earlier pre-sync full run, but passed immediate focused runs
  `3791f7ae-e65f-423c-b440-28fdf024fff0` (25.315 s) and
  `19671636-df32-4678-8357-3feeab28aa7d` (29.626 s), so the result is recorded
  as overlap-sensitive under the planner's direction.

Earlier pre-sync full runs were also not green and remain history. Run
`beb14a32-e141-4f87-9502-862f895b44ea` collected 6,161 tests, with 6,159
passing, two failing and 31 skipped; the nested-remote deadline and gateway
TLS cases each passed their immediate focused rerun (`03aee710-466a-4998-a98a-094827a3884c`,
3.633 s; `3791f7ae-e65f-423c-b440-28fdf024fff0`, 25.315 s). An earlier run
`f1b905bb-4fb0-4895-b054-0d31028cf26a` had attributed the nested-remote result
to task `c9bac288` after one focused failure; the later exact focused pass
does not establish that ownership, so that attribution is withdrawn. Its
`racing_invocations_with_one_run_id_start_one_run` failure also passed its
immediate focused run `34cf1180-ef25-45eb-9c0c-4f016b455487` (12.897 s).

The earlier fault aggregate run `9d2c45ee-6bf7-4d50-913e-18f3305573c6`
passed 15/18 in the faults selection while load ranged from 50 to 115 and
other fault selectors were active; this is unproven under contention, not a
pass. A later high-load diagnostic aggregate run
`daeef291-cb9e-4ad6-b936-d55b5b243ccc` passed 18/18 faults cases and all
following selections, with other Effigy test activity still running. Per the
planner's direction, that diagnostic does not replace quiet aggregate proof;
the planner will run it in the next milestone QA after merge. The only fault
change is build wiring in `config/tasks.toml:400-410`: one custody-fixture
binary is built once and reused. The feature flags, filters, assertions,
fault-scenario runtime behavior and timeout profiles are unchanged.
