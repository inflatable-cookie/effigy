# Noninteractive Init Actions And Checklist

Date: 2026-05-19  
Roadmap: [`g07.054`](../../../roadmaps/g07/054-noninteractive-init-action-execution-and-migration-paths.md)  
Batch card: [`1004`](../../../roadmaps/g07/batch-cards/1004-add-noninteractive-action-execution-and-migration-flows.md)  
Strict lane: [`093`](../../../specs/093-init-setup-wizard-strict-lane.md)

## What Changed

- widened builtin init mode parsing with:
  - `--checklist`
  - `--apply-actions <ID>[,<ID>...]`
- added a machine-readable checklist response under
  `effigy.init.checklist.v1`
- added an explicit action-execution response under
  `effigy.init.actions.v1`
- wired non-interactive action execution through the shared setup inventory so
  init can now drive:
  - baseline managed setup jobs
  - builtin/task-backed checks such as `doctor`, `tasks`, and `test --plan`
  - top-level command surfaces such as `graph`, `bundle`, and `secrets`
- kept guidance-only jobs honest instead of faking execution support
- preserved baseline init behavior:
  - plain default apply path still exists
  - `--check`, `--apply`, and `--repair` still mean baseline init work

## Why This Matters

- agents now have the same setup-job surface as the TTY wizard without prompt
  handling
- setup execution is explicit and scriptable instead of inferred from TTY-only
  flows
- per-action reporting makes partial progress and blocked jobs visible instead
  of collapsing everything into one init result

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Moved:
  - init can now expose one shared setup inventory to humans and agents
  - JSON contracts exist for both checklist planning and selected-action
    execution
  - top-level command-backed setup jobs no longer depend on pretending they are
    manifest tasks
- Remaining open:
  - broader user-facing docs and closeout proof remain in `1005`

## Validation

- `cargo test -p effigy-builtin checklist_and_apply_actions_parse_as_distinct_modes -- --nocapture`
- `cargo test builtin_init_checklist_json_contract_has_versioned_shape -- --nocapture`
- `cargo test builtin_init_actions_json_contract_has_versioned_shape -- --nocapture`
- `cargo test run_manifest_task_builtin_init_checklist_json_reports_setup_inventory -- --nocapture`
- `cargo test run_manifest_task_builtin_init_apply_actions_json_reports_applied_and_guided_outcomes -- --nocapture`
- `cargo test run_manifest_task_builtin_init_apply_actions_can_run_nested_graph_status -- --nocapture`
- `cargo clippy -p effigy-builtin --tests -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`

## Next Task

Execute `1005`.
