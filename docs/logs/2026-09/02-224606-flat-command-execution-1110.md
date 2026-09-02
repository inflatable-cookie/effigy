# Flat Command Execution 1110 Closeout

Status: complete
Created: 2026-09-02
Roadmap: `g09.002`
Spec: `117` (archived with this closeout)
Card: `1110`
Branch: `worker/g09-002-flat-command-execution-1110`
PR: pending exact-head review
Review head: `worker/g09-002-flat-command-execution-1110` tip at submission

## Outcome

Direct built-in invocation is canonical again. The five executable namespace
aliases from card `1109` (`local`, `repo`, `deliver`, `extend`, `admin`) and
their `legacy-direct-command` migration surface are gone. Grouped help and
`effigy help <group>` remain discovery-only. Former namespace words return to
ordinary selector routing. Genuine command-owned subcommands stay nested.

## Deleted Alias And Warning Surfaces

- `crates/effigy-cli/src/command_parsing.rs` no longer parses space-separated
  namespace prefixes; those first words fall through to `parse_task_command`
- `Command::GroupedBuiltin` and `ExecutionSurface::GroupedBuiltin` are removed
- `src/cli/legacy_direct.rs` is deleted
- JSON envelopes no longer grow a `warnings` array for this preview
- Help, completion, current guides, config examples, generated references, and
  both managed-skill copies teach `effigy <command> ...`

Historical preview records kept as written: `g09.001`, card `1109`, archived
spec `116`, and evidence `docs/logs/2026-09/02-205536-command-surface-preview-1109.md`.

## Review Oracle Mapping (card `1110` / spec `117`)

| Oracle row | Named proof |
| --- | --- |
| 1. any direct built-in changes inner text/JSON, side effects, command identity, arguments, or exit apart from warning removal | Restored pre-preview parsers/dispatch/output; `tests/cli_output_tests/flat_command_execution_tests.rs` (`direct_text_and_json_success_have_no_migration_metadata`, `direct_usage_and_runtime_errors_have_no_migration_metadata`, `graph_watch_json_stream_has_no_migration_stderr`); envelope unit tests in `src/cli/output/envelope/tests.rs` assert the pre-warning `effigy.command.v1` shape |
| 2. any former namespace word remains reserved or fails to execute a same-named manifest task with its arguments | `src/tests/lib_tests_parse_tests/selector_restoration_tests.rs::former_namespace_words_parse_as_task_selectors_and_keep_following_args`; `flat_command_execution_tests.rs::former_namespace_words_run_same_named_manifest_tasks_with_their_args` |
| 3. an unowned grouped spelling still reaches the built-in child | `selector_restoration_tests.rs::unowned_former_grouped_spellings_do_not_parse_as_the_child_builtin`; `flat_command_execution_tests.rs::unowned_former_grouped_spelling_does_not_reach_the_child_builtin` |
| 4. general help is no longer grouped or a focused `help <group>` view regresses | `src/tests/lib_tests_help_render_tests.rs` (`render_general_help_groups_commands_by_operator_job`, `render_group_help_covers_every_group_without_execution_grammar`, `render_repo_group_help_lists_only_repository_intelligence_commands`); `tests/cli_output_tests/help_and_flags_tests.rs` (`cli_general_help_renders_the_six_operator_groups`, `cli_help_repo_group_lists_only_repository_intelligence_commands`, `cli_help_group_json_mode_emits_machine_readable_payload`); `crates/effigy-cli/src/command_surface/tests.rs::group_inventories_match_the_contract_taxonomy` |
| 5. help, completion, current docs, generated references, or managed skills still teach namespace-prefixed execution | `command_surface/tests.rs::general_help_rows_use_direct_spellings_not_namespace_prefixes`; `crates/effigy-builtin/.../command_index.rs::completion_first_tokens_are_direct_commands_not_help_group_prefixes`; `tests/documentation_coverage_tests.rs::current_guidance_does_not_teach_namespace_prefixed_execution` plus `project_local_and_distributed_effigy_skills_have_semantic_parity` and `help_first_discovery_paths_are_documented` |
| 6. any direct route emits a migration warning/note or unrelated JSON output retains preview-only warning state | `flat_command_execution_tests.rs` success/error/stream cases plus `assert_no_migration_diagnostics` / `assert_json_has_no_warning_metadata`; `src/cli/output/envelope.rs` has no `warnings` field |
| 7. repository shadowing, slash selectors, leading global flags, or a genuine subcommand changes | `flat_command_execution_tests.rs` (`shadowed_deferred_builtin_keeps_manifest_precedence`, `catalog_slash_alias_stays_a_selector`, `leading_repo_and_json_flags_still_select_direct_builtins`, `genuine_subcommands_keep_nested_help`); `selector_restoration_tests.rs` (`slash_selectors_never_enter_help_group_or_builtin_parsing`, `genuine_command_owned_subcommands_stay_nested`); `help_and_flag_tests::parse_help_group_words_stay_available_to_task_selectors`; `help_and_flags_tests::cli_manifest_selector_named_after_a_help_group_keeps_task_routing` |
| 8. archived preview evidence is rewritten rather than superseded by current authority | `g09.001`, card `1109`, archived spec `116`, and `02-205536` bodies unchanged; this log supersedes them as current authority |

Smallest counterexample set coverage: one direct child from each former group
(`graph`, `container`, `release`, `skill`/`config` via help inventory and CLI
success/error fixtures); all five former namespace words as manifest tasks;
one unowned `repo graph` route; general and focused help snapshots; completion
first-token inventory; text/JSON success, usage-error, runtime-error, and
`graph watch` stream; a shadowing `docs` task; `docs context`, `release gates`,
and `service pack` nested help.

## Remaining Shadowing Limitation

Deferred built-ins can still be shadowed by a same-named repository task.
Card `1110` did not add a replacement for the preview's grouped escape
(`effigy repo docs` is no longer a built-in route). Existing selector
precedence is unchanged. This repository's documentation QA invokes the docs
builtin through `cargo run --bin effigy -- docs ...`, not through a shadowed
selector.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`
- Movement: executable help-taxonomy aliases with migration warnings ->
  canonical direct invocation, discovery-only grouping, restored selector
  routing for the five former namespace words
- Remaining gap: deferred-builtin shadowing (existing precedence, no new
  escape); Effigy release and S3 extraction remain separately gated

## Validation Performed

- `cargo test --lib parse_tests::selector_restoration` — 4 passed
- `cargo test -p effigy-cli command_surface` — 13 passed
- `cargo test --lib help_render` — 29 passed
- `cargo test --lib help_deferral` — 4 passed
- `cargo test --lib parse_tests::help_and_flag` — 45 passed
- `cargo test --test cli_output_tests flat_command` — 9 passed
- `cargo test --test cli_output_tests help_and_flags` — 26 passed
- `cargo test --test documentation_coverage_tests` — 6 passed
- `cargo test -p effigy-builtin completion_first_tokens` — 1 passed
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `git diff --check` — clean
- `effigy qa` — 3739 passed, 1 skipped; docs (links, examples, index, agent-defaults, vision headings/contains/workflow-paths/index/next-action) passed; JSON contract checks passed
- `effigy doctor --json` — exit 0, `effigy.command.v1` ok envelope; seven pre-existing god-file warnings only; no `warnings` migration field
- Northstar rust-quality compact closeout (`RUST-READ-001`, `RUST-API-001`,
  `RUST-ERR-001`): compiler/lint/test requests completed; compact status is
  `warning` from the existing `proc-macro-error2` future-incompat note, not
  from this lane's code

Known unrelated flake: `cli_container_attached_session_handles_sigint_during_startup`
failed on unmodified `main` during the `1109` lane; do not treat an isolated
failure of that test as this rollback's regression.

## Risks

- Repositories that own a deferred built-in name still cannot reach that
  built-in without changing the manifest. That is preserved policy, not a
  regression invented here.
- Historical preview docs still mention grouped spellings. Current guidance
  and generated references do not.

## Next Task

Return the exact PR head to the Effigy orchestrator for review and merge.
Do not merge from this worker.
