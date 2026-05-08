# 607 - Add State Capture Artifact Stage Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Let `effigy state capture --yes` stage an already-produced capture payload as a
local artifact and embed the artifact capture report in
`effigy.state-stack.capture.v1`.

## Scope

- add an explicit payload source input for state capture execution
- require `--yes` before staging any capture artifact
- reuse the existing artifact capture substrate
- keep OCI publish out of this card
- keep repo-owned capture tasks as planned-only unless a later hook card adds
  execution
- update JSON/text reports so planned and staged captures are clearly distinct

## Non-Goals

- no OCI push
- no database/media capture generation
- no repo hook execution
- no data diff engine
- no conflict detection
- no release work

## Exit Condition

This card is complete when a local payload can be staged through
`state capture --yes --source <PATH>` and the state capture report embeds the
artifact capture report without claiming app-specific capture semantics.

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy --lib state -- --nocapture`
- `cargo test --test cli_output_tests state_command_tests::cli_state_capture -- --nocapture`
- `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- `cargo run --bin effigy -- docs check-paths docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/batch-cards/607-add-state-capture-artifact-stage-boundary.md docs/roadmaps/g04/batch-cards/608-add-state-capture-explicit-oci-publish-boundary.md`
- `git diff --check`

## Next Task

Add explicit OCI publish for already-staged state capture artifacts.
