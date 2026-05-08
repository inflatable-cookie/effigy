# 606 - Add Plan-Only State Capture Command Surface

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Add a plan-only `effigy state capture` command that emits
`effigy.state-stack.capture.v1` without executing capture hooks or staging
new artifacts.

## Scope

- add CLI parsing and help for `effigy state capture`
- load the selected composed or standalone state stack using the existing state
  manifest path
- accept capture role, source environment, produced layer key, and optional
  destination ref inputs
- emit the planned capture report shape from the contract
- report repo-owned capture tasks as `planned`
- keep execution unavailable until a later card

## Non-Goals

- no capture execution
- no artifact capture/stage/push
- no data diff engine
- no media mutation
- no conflict detection
- no release work

## Exit Condition

This card is complete when `effigy --json state capture ...` returns a valid
plan-only capture report and text output makes clear that nothing was executed.

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy --lib state -- --nocapture`
- `cargo test --test cli_output_tests state_command_tests::cli_state_capture -- --nocapture`
- `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- `cargo run --bin effigy -- docs check-paths docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/guides/017-json-output-contracts.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/batch-cards/606-add-plan-only-state-capture-command-surface.md docs/roadmaps/g04/batch-cards/607-add-state-capture-artifact-stage-boundary.md`
- `git diff --check`

## Next Task

Add the capture artifact staging boundary while keeping publish explicit and
app-specific diff generation repo-owned.
