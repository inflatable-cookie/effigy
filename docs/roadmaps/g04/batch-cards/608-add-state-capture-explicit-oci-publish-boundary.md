# 608 - Add State Capture Explicit OCI Publish Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Allow state capture to publish an already-staged capture artifact to an explicit
OCI ref without adding repo-owned capture hook execution.

## Scope

- add an explicit publish flag for `state capture`
- require an explicit `oci://` destination ref
- reuse the artifact capture push path
- embed pushed digest/descriptor details in `effigy.state-stack.capture.v1`
- keep local staging as the first phase before publish
- keep app-specific payload generation repo-owned

## Non-Goals

- no repo hook execution
- no database/media capture generation
- no conflict detection
- no background sync
- no release work

## Exit Condition

This card is complete when `state capture --yes --push --source <PATH> --ref
oci://...` stages locally, publishes explicitly, and reports the digest without
claiming capture generation semantics.

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy --lib state -- --nocapture`
- `cargo test --test cli_output_tests state_command_tests::cli_state_capture -- --nocapture`
- `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- `cargo run --bin effigy -- docs check-paths docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/batch-cards/608-add-state-capture-explicit-oci-publish-boundary.md docs/roadmaps/g04/batch-cards/609-add-state-capture-repo-task-execution-boundary.md`
- `git diff --check`

## Next Task

Add repo-owned capture task execution.
