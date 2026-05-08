# 600 - Add State Stack Lineage Report Location Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Add a durable operator-visible report location for `effigy state plan` output
before introducing layer execution.

## Scope

- define the repo-local report path for state-stack lineage plans
- add an explicit flag to write the current plan report to that location
- keep stdout JSON/text behavior unchanged unless the write flag is supplied
- report the written path in text and JSON-friendly output
- update docs and tests for the report contract

## Non-Goals

- no layer apply execution
- no capture execution
- no app hook execution
- no append-only lineage ledger
- no OCI artifact staging or pulling from `state plan`
- no release work

## Exit Condition

This card is complete when an operator can run `effigy state plan --write-report`
and get a stable report artifact under Effigy-owned repo-local state without
changing the plan-only execution boundary.

## Validation

- PASS: `cargo fmt --all -- --check`
- PASS: `cargo test -p effigy-state -- --nocapture`
- PASS: `cargo test -p effigy --lib state -- --nocapture`
- PASS: `cargo test --test cli_output_tests state_command_tests -- --nocapture`
- PASS: `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- PASS: `cargo run --bin effigy -- docs check-paths CHANGELOG.md docs/guides/017-json-output-contracts.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/roadmaps/g04/batch-cards/600-add-state-stack-lineage-report-location-boundary.md docs/roadmaps/g04/batch-cards/601-add-state-stack-task-apply-adapter-boundary.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md`
- PASS: `git diff --check`

## Next Task

Start
[`601-add-state-stack-task-apply-adapter-boundary.md`](./601-add-state-stack-task-apply-adapter-boundary.md).
