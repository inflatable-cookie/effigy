# 599 - Add State Stack Manifest Config Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Define and implement the first repo-native composed-manifest boundary for
state-stack config so `effigy state plan` does not stay trapped as a path-only
proof.

## Scope

- support `[state]` in the composed Effigy manifest
- support `[state].default_stack` and `[state.stacks.<name>]`
- support `effigy state plan --stack <NAME>`
- keep explicit `effigy state plan <MANIFEST>` standalone behavior unchanged
- report clear errors when `[state]` is missing or stack selection is ambiguous
- update command help and docs for the manifest config contract

## Non-Goals

- no layer apply execution
- no capture execution
- no app hook execution
- no durable lineage ledger
- no record-level reconciliation
- no release work

## Exit Condition

This card is complete when a repo can declare state-stack config in the composed
Effigy manifest and `effigy state plan` can produce the same lineage report
without a positional manifest path.

## Validation

- PASS: `cargo fmt --all -- --check`
- PASS: `cargo test -p effigy-manifest state -- --nocapture`
- PASS: `cargo test -p effigy --lib state -- --nocapture`
- PASS: `cargo test --test cli_output_tests state_command_tests -- --nocapture`
- PASS: `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- PASS: `cargo run --bin effigy -- docs check-paths CHANGELOG.md docs/guides/025-command-reference-matrix.md docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/roadmaps/g04/batch-cards/599-add-state-stack-manifest-config-boundary.md docs/roadmaps/g04/batch-cards/600-add-state-stack-lineage-report-location-boundary.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md`
- PASS: `git diff --check`

## Next Task

Start
[`600-add-state-stack-lineage-report-location-boundary.md`](./600-add-state-stack-lineage-report-location-boundary.md).
