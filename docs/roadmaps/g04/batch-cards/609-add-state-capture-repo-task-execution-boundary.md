# 609 - Add State Capture Repo Task Execution Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Allow `state capture --yes --task <TASK>` to run one repo-owned capture task
before artifact staging, without moving app-specific capture generation into
Effigy.

## Scope

- execute the named repo task through existing Effigy task execution
- pass structured state capture context through environment variables
- keep the task responsible for producing the payload file
- require `--source <PATH>` to identify the task output payload after execution
- preserve local stage and optional explicit push behavior after the task
- record task output or failure in `effigy.state-stack.capture.v1`

## Non-Goals

- no built-in database/media diff generation
- no app-specific capture validation
- no conflict detection
- no background sync
- no release work

## Exit Condition

This card is complete when a repo-owned capture task can produce or update a
payload file, `state capture --yes` can stage that payload afterward, and the
state capture report records both task and artifact results.

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy --lib state -- --nocapture`
- `cargo test --test cli_output_tests state_command_tests::cli_state_capture -- --nocapture`
- `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- `cargo run --bin effigy -- docs check-paths docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/batch-cards/609-add-state-capture-repo-task-execution-boundary.md docs/roadmaps/g04/batch-cards/610-add-state-lineage-history-lookup-design-boundary.md`
- `git diff --check`

## Next Task

Design lineage-history lookup before adding more execution semantics.
