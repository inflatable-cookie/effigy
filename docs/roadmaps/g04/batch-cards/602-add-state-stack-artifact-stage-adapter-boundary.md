# 602 - Add State Stack Artifact Stage Adapter Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Add artifact staging to state-stack apply reports without applying artifact
payloads to databases or media stores.

## Scope

- stage `apply_mode = "artifact"` layers through the existing artifact staging
  substrate
- keep task execution behavior from `601`
- include staged artifact paths and metadata refs in the apply report
- preserve `--yes` as the execution gate
- leave SQL import and app hook application to later adapters

## Non-Goals

- no SQL import execution
- no media library mutation
- no capture adapter
- no app-specific migration logic
- no record-level reconciliation
- no release work

## Exit Condition

This card is complete when `effigy state apply --yes` can execute task layers
and stage artifact layers while still refusing to apply artifact payload
semantics.

## Validation

- PASS: `cargo fmt --all -- --check`
- PASS: `cargo test -p effigy --lib state -- --nocapture`
- PASS: `cargo test --test cli_output_tests state_command_tests -- --nocapture`
- PASS: `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- PASS: `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- PASS: `cargo run --bin effigy -- docs check-paths CHANGELOG.md docs/contracts/json-schema-index.json docs/guides/017-json-output-contracts.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/roadmaps/g04/batch-cards/602-add-state-stack-artifact-stage-adapter-boundary.md docs/roadmaps/g04/batch-cards/603-add-state-stack-sql-apply-design-boundary.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md`
- PASS: `git diff --check`

## Next Task

Start
[`603-add-state-stack-sql-apply-design-boundary.md`](./603-add-state-stack-sql-apply-design-boundary.md).
