# 601 - Add State Stack Task Apply Adapter Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Add the first bounded execution adapter for state stacks by applying only
`apply_mode = "task"` layers through existing Effigy task execution.

## Scope

- add a plan-first apply surface for task-mode layers
- require an explicit operator flag before execution
- preserve state-stack ordering and dependency validation
- skip non-task layers with clear planned/unsupported output
- include task execution results in a state-stack execution report
- keep app semantics inside repo-owned tasks

## Non-Goals

- no OCI artifact staging or pulling
- no SQL import adapter
- no capture adapter
- no rebase/conflict resolution
- no app-specific migration logic
- no release work

## Exit Condition

This card is complete when Effigy can execute ordered `task` layers from a
validated state stack while leaving artifact, SQL, capture, and app-specific
semantics unexecuted.

## Validation

- PASS: `cargo fmt --all -- --check`
- PASS: `cargo test -p effigy --lib state -- --nocapture`
- PASS: `cargo test --test cli_output_tests state_command_tests -- --nocapture`
- PASS: `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- PASS: `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- PASS: `cargo run --bin effigy -- docs check-paths CHANGELOG.md docs/contracts/json-schema-index.json docs/guides/017-json-output-contracts.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/roadmaps/g04/batch-cards/601-add-state-stack-task-apply-adapter-boundary.md docs/roadmaps/g04/batch-cards/602-add-state-stack-artifact-stage-adapter-boundary.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md`
- PASS: `git diff --check`

## Next Task

Start
[`602-add-state-stack-artifact-stage-adapter-boundary.md`](./602-add-state-stack-artifact-stage-adapter-boundary.md).
