# 604 - Add State Stack SQL Import Adapter Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Implement the narrowest safe SQL apply adapter for state-stack layers by
reusing existing database seed/import plumbing.

## Scope

- add a layer-level SQL target field or compatible manifest extension
- resolve SQL targets through existing `[data.targets]` and bundle database
  target logic
- stage local or OCI SQL payloads before import
- require `state apply --yes` before SQL execution
- embed artifact staging and SQL import reports in `effigy.state-stack.apply.v1`
- fail before execution when SQL target selection is ambiguous

## Non-Goals

- no app-specific transform or validation logic
- no record-level reconciliation
- no media mutation
- no capture adapter
- no live sync behavior
- no release work

## Exit Condition

This card is complete when a SQL state layer can import a staged SQL payload
into one explicitly resolved generated-compose database target without adding a
new database abstraction.

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy-state -- --nocapture`
- `cargo test -p effigy --lib state -- --nocapture`
- `cargo test --test cli_output_tests state_command_tests -- --nocapture`
- `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- `cargo run --bin effigy -- docs check-paths docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/batch-cards/604-add-state-stack-sql-import-adapter-boundary.md docs/roadmaps/g04/batch-cards/605-add-state-stack-capture-report-design-boundary.md`
- `git diff --check`

## Next Task

Move to
[`605-add-state-stack-capture-report-design-boundary.md`](./605-add-state-stack-capture-report-design-boundary.md).
