# Demo Lifecycle Control Foundation Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.9`

## Summary

Shipped the first honest lifecycle-control slice for Effigy's demo runner on
top of the existing registry, inspection, and run foundation.

Delivered in this batch:

- runner-owned active-attempt state for in-flight demos
- `effigy demo stop <id>` for directly runner-owned run-backed demos
- `effigy demo rerun <id>` as a fresh-attempt command
- `demo inspect` output that distinguishes active work from the latest
  terminal receipt
- explicit error reporting for task-backed demos that are runnable but not yet
  stoppable through the current runtime

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `demo execution without honest lifecycle control` to
  `runner-owned active attempts plus bounded stop/rerun control`
- Remaining open:
  - broader stoppability/runtime expansion
  - browser-facing state polish
  - later TUI/browser implementation

## Validation

- `cargo test cli_demo_ --test cli_output_tests`
- `cargo test parse_demo_rerun_with_repo_and_json -- --exact`
- `cargo test cli_demo_help_is_command_specific --test cli_output_tests -- --exact`

## Outcome

The runner now has one explicit active-attempt model instead of inferring all
lifecycle from terminal receipts alone. The stop boundary is honest: run-backed
demos can be signaled through a runner-owned handle, while task-backed demos
report the current limitation explicitly instead of pretending cancellation
exists.

## Next Task

Use the next `g02.003` ready card to decide whether the follow-up should
prioritize browser-facing state polish or broader stoppability/runtime
expansion.
