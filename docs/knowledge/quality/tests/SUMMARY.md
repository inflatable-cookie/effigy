# Test ratings at `b5b54db3538b57416f8cc1b8d0fb1e8ec0d0bae8`

The tables contain 7,724 rows: 6,458 Rust and 1,266 Python tests. Thirty ignored helper entrypoints
are marked `helper entrypoint` and excluded from scoring, leaving 7,694 scored tests.

## Distribution (scores 0–10)

| Score | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Tests | 8 | 29 | 82 | 1,574 | 2,417 | 2,796 | 788 |

Mean 8.33; median 8. 0.5% scored 5 or lower and 1.5% scored 6 or lower.

## Per-domain results

| Domain | Table rows | Scored tests | Mean | Score ≤ 6 |
|---|---:|---:|---:|---:|
| T1 containers | 1,317 | 1,316 | 7.83 | 30 |
| T2 gateway/host/managed | 627 | 625 | 8.32 | 13 |
| T3 kernel/custody | 858 | 837 | 8.59 | 3 |
| T4 CLI/config/engine/UI | 1,797 | 1,795 | 7.77 | 67 |
| T5 other Rust | 1,859 | 1,857 | 8.69 | 6 |
| T6 Python harness | 1,266 | 1,264 | 8.92 | 0 |

## Why 132 rows still sit below 7

119 of them are scored tests; 13 are excluded helper entrypoints.

| Primary reason | Rows |
|---|---:|
| Weak oracle: the assertion is a substring, presence check or thin delegate where the exact outcome is available | 64 |
| Presentation echo: pinned progress, help, prompt or colour text carries almost no behaviour (V=1) | 39 |
| Overlap: a sharp row inside a family one sibling already covers (U=1) | 8 |
| Fixture or helper entrypoint: no behaviour claim of its own, most `#[ignore]`d | 21 |

No row below 7 loses only cost. Every full-value test that must cross a real boundary
(3 value, 3 effectiveness, 2 uniqueness, 0 cost) scores 8.

## Improvable rows

These are not boundary or helper exceptions; each would reach 7 with one concrete change.

- `effigy_manifest::composition::tests::extend_on_non_conflicting_path_is_silent_noop` — assert the merged array, not that the load succeeded.
- `effigy_manifest::composition::tests::manifest_minimum_effigy_version_accepts_current_version` — assert the recorded/accepted version rather than a successful load.
- `effigy_rhai::tests::utility::execute_rhai_script_allows_explicit_effigy_binary_paths` — assert the child ran (marker/output), not only that the call returned `Ok`.
- `effigy_tui::terminal_text::tests::vt_logs_trims_empty_padding_lines` — assert the trimming the name claims, not that two lines survive.
- `effigy_builtin::completion::prompt::tests::completion_shell_prompt_without_default_stays_well_formed` — assert the numbered choices and the no-default path, not one literal.
- `effigy::tests::runner_tests::tasks_listing_tests::catalog_and_builtin_text_tests::run_tasks_lists_catalogs_and_tasks` — compare the canonical tail instead of three substrings.
- `…::catalog_and_builtin_text_tests::run_tasks_without_catalogs_still_lists_builtin_tasks` — compare the builtin section, not five marker strings.
- The seven `…::filter_and_managed_tests::run_tasks_with_*` rows — assert the whole filtered block as the canonical-table rows do.
- `…::builtin_command_tests::overrides_and_config_tests::config_reference_tests::run_manifest_task_builtin_config_prints_reference` — parse the reference and assert its sections.
- `…::config_builtin_tests::formatting_snapshot_tests::run_manifest_task_builtin_config_has_blank_line_between_sections` — assert the rendered section sequence.
- The ten `cli_output_tests::help_and_flags_tests::*` and `tasks_rendering_tests::*help*` rows — compare the binary panel with the in-process panel table exactly; today they overlap it with substring checks.
- `effigy::runner::workspace_command::staged_probe_tests::identity_refuses_a_reply_from_another_executable` — assert the typed reason, not `reason.contains(...)`.
- The five `…::reserved_creator_binding::creator_binding_*_refuses_before_publication` rows — assert the typed refusal and zero publication rows.
- `cli_output_tests::command_behavior_tests::shared_cli_container_up_cohort` — assert the cohort's output and route, not its exit status alone.
- `cli_output_tests::command_behavior_tests::cli_container_data_seed_json_reports_seed_contract` — give it its own assertions or fold it into the helper it delegates to.
- `…::gateway_registration::tests::gateway_readiness_reports_each_poll_and_its_blocker` and `…::gateway_readiness_waits_for_declared_service_ports_before_returning_rows` — assert the exact poll sequence and its blocker.
- `gateway_private::detached_gateway_failure_actor` and `effigy-gateway::server::tests::milestone_130acca8_notification_wakes_the_route_worker` — assert the applied snapshot, not a wake count.
- The four `…::managed_and_locking_tests::managed_tui_tests::*` plan rows — assert a whole plan block instead of the lines appearing.
- The four `cli_output_tests::command_behavior_tests::cli_container_{cache,data-export,data-import,data-list}_json_*` rows — share one schema fixture with the exact field assertions instead of four separate binary runs.

## Reading

Round 4 (tasks #910–#918) raised the low tail and consolidated redundant cases: 422 rows left the
tables because their tests are gone, 151 merged or new survivors arrived, and 865 rows were
re-rated against the clarified cost rule. What remains below the floor is presentation echo and
substring oracles, not lost custody, refusal, secret or identity coverage; the safety families in
T1–T3 and T6 score 8–10.
