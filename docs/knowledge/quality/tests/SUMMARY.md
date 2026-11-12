# Test ratings at `5db5437b48de595c5a597036da4c7013ef658b74`

The tables contain 7,701 rows: 6,437 Rust and 1,264 Python tests. Thirty parent-invoked helper and
child entrypoints are marked `helper` in the `excluded` column and excluded from scoring, leaving
7,671 scored tests.

## Distribution (scores 0–10)

| Score | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Tests | 5 | 7 | 27 | 1,617 | 2,429 | 2,797 | 789 |

Mean 8.35; median 8. 0.2% scored 5 or lower and 0.5% scored 6 or lower.

## Per-domain results

| Domain | Table rows | Scored tests | Mean | Score ≤ 6 |
|---|---:|---:|---:|---:|
| T1 containers | 1,306 | 1,305 | 7.85 | 18 |
| T2 gateway/host/managed | 623 | 621 | 8.34 | 8 |
| T3 kernel/custody | 858 | 837 | 8.59 | 3 |
| T4 CLI/config/engine/UI | 1,793 | 1,791 | 7.83 | 6 |
| T5 other Rust | 1,857 | 1,855 | 8.69 | 4 |
| T6 Python harness | 1,264 | 1,262 | 8.92 | 0 |

## Why 39 scored tests still sit below 7

No row below 7 loses only cost. Every full-value row that must cross a real boundary
(3 value, 3 effectiveness, 2 uniqueness, 0 cost) scores 8. The remaining low tail is overlap, weak
oracles, presentation echo and fixture children.

| Primary reason | Rows |
|---|---:|
| Overlap: a sharper sibling already proves the claim (U=1) | 17 |
| Fixture or child entrypoint with no behaviour claim of its own | 9 |
| Weak oracle: substring, thin delegate, partial assertion or an unmeasured budget (E≤2 with no overlap) | 7 |
| Presentation echo: pinned progress, help or colour text carries almost no behaviour (V=1) | 4 |
| Ignored or env-gated proof that never runs in the default suite | 2 |

The overlap rows are the reserved-creator-binding refusals (5), the gateway readiness poll cases
(3), the bounded-exec refusal-accounting cases (3), the two `milestone_130acca8` wake/retry cases,
`bootstrap_help_is_command_specific` and three resolver/deadline siblings. The fixture rows are the
gateway adapterless, watchdog and listener children plus the CLI/workspace fixture children and the
kernel crash fixture.

## Improvable rows

These are not boundary or helper exceptions; each would reach 7 with one concrete change.

- `effigy::runner::container_command::gateway_registration::tests::gateway_readiness_reports_each_poll_and_its_blocker`
  and `…::gateway_readiness_waits_for_declared_service_ports_before_returning_rows` — assert the exact poll sequence and its blocker.
- The five `…::reserved_creator_binding::creator_binding_*_refuses_before_publication` rows — assert the typed refusal and zero publication rows, then keep one table entry.
- `…::bounded_exec::tests::refusal_accounting_pending_progress_is_a_lower_bound_not_zero`,
  `…::refusal_accounting_secret_partial_output_never_reaches_any_surface` and
  `…::stderr_overflow_names_the_stderr_stream` — fold into the retained refusal-accounting rows that already own the stream and redaction claims.
- `…::tests::reserved_input::a_stalled_fragment_stops_the_closure_at_the_deadline` and
  `…::reserved_input::reserved_input_fixture_waits_for_an_ambient_path_mutator` — assert the typed reason, not the shared path-mutator helper's verdict.
- `…::workspace_command::tests::sibling_effigy_repo_root_discovers_adjacent_and_nested_projects_checkouts`
  — assert the resolved project roots, not their presence.
- `effigy-gateway::server::tests::milestone_130acca8_notification_wakes_the_route_worker` and
  `…::milestone_130acca8_refused_observation_retries_the_committed_snapshot` — assert the applied snapshot, not a wake count or retry count.
- `gateway_private::detached_gateway_failure_actor` and `gateway_private::adapterless_listener::managed_host_adapterless_cleanup_stops_a_descendant_captured_before_its_leader_stopped`
  — assert the recorded owner record, not that the fixture client exited.
- `effigy-kernel::custody::anchor_spawn_overhead_is_measured` — give the anchor spawn a threshold or move it behind `#[ignore]` as a benchmark; today it measures and asserts nothing.
- `cli_output_tests::command_behavior_tests::shared_cli_container_up_cohort` — assert the cohort's route and output, not only that its three cases reported `ok`.
- `effigy::tests::bootstrap_cli_tests::bootstrap_help_is_command_specific` and
  `…::v0141_bootstrap_fixtures_load_and_list_tasks` — compare the typed help panel and the listed task rows instead of substrings.
- `…::managed_and_locking_tests::managed_supervisor_fault_tests::supervisor_fault_shard` — assert each armed fault's recorded outcome, not that a shard ran.

## Reading

Round 4 (tasks #910–#918) raised the low tail and consolidated redundant cases. The round 4b
close (tasks #922/#923, PRs #957–#959) then touched 65 rows, which are re-read here: 57 were
raised, 7 kept their score, and one fell by the clarified cost rule
(`cli_release_plan_json_mode_reports_the_whole_pipeline_without_side_effects` is a full CLI run,
so its cost is 0 rather than 1). Nineteen merged or new survivors arrived, and 36 rows left the
tables, either because their test was merged into a single entrypoint or because the item is now a
plain helper. Six more T6 rows left because they named a test under a class that never declared or
inherited it, duplicating the row for the class that does. Consolidation lowered the table row count without dropping logical scenarios: each
`*_contract_cases` entrypoint invokes the survivors it absorbed. 51 rows moved from below the
floor to 7 or above, so 39 scored tests remain below 7 instead of 119.

The remaining low tail is overlap and fixture machinery, not lost custody, refusal, secret or
identity coverage: the safety families in T1–T3 and T6 score 8–10. The largest single improvement
this round came from asserting exact typed panels, typed refusals and whole rows instead of
substring fragments.
