# Roadmap 012 Batch 12.1 Checkpoint (Builtin Test Consolidation)

Date: 2026-03-01
Roadmap: [g01.012 - Codebase Consolidation and Health](../../../roadmaps/g01/012-codebase-consolidation-and-health.md)

## Scope

Refactor `src/runner/builtin/test.rs` to reduce branching duplication and separate concerns without changing command behavior.

## Changes

- Extracted runnable target expansion into `collect_builtin_test_runnable_targets`.
- Extracted suite selection and ambiguity handling into `select_builtin_test_suite`.
- Consolidated plan-recovery vs invocation-error routing into `render_suite_selection_failure`.
- Extracted plan rendering into `render_builtin_test_plan`.
- Extracted passthrough command augmentation into `apply_passthrough_to_runnable`.
- Centralized available-suite string rendering with `render_available_suites`.

## Validation

Executed targeted tests:
- `cargo test --lib run_manifest_task_builtin_test_plan_ -- --nocapture`
- `cargo test --lib runner::tests::run_manifest_task_builtin_test_with_named_args_errors_when_multi_suite_is_ambiguous -- --exact`
- `cargo test --lib runner::tests::run_manifest_task_builtin_test_mistyped_suite_suggests_nearest_runner -- --exact`
- `cargo test --lib runner::tests::run_manifest_task_builtin_test_failure_with_suite_filter_shows_no_match_hint -- --exact`

Result: all targeted tests passed.

## Notes

This batch intentionally keeps literal output strings and JSON schema payload shapes unchanged.
