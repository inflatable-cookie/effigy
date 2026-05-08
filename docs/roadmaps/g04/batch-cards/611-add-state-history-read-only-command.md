# 611 - Add State History Read-Only Command

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Add a read-only `effigy state history` command that scans state report files
for a stack and returns a bounded history payload.

## Scope

- add `effigy state history --stack <NAME>`
- scan `.effigy/reports/state/<stack>/`
- support existing `plan.json` and future `latest-*` / `history/*.json`
  layouts
- support `--kind plan|apply|capture`, `--limit <N>`, and `--lineage <ID>`
- ignore malformed JSON files with warnings
- emit `effigy.state-stack.history.v1`

## Non-Goals

- no report writing changes
- no retention or pruning
- no database-backed ledger
- no conflict detection
- no release work

## Exit Condition

This card is complete when operators can query existing state report files for a
stack without requiring a new persisted index.

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy --lib state -- --nocapture`
- `cargo test --test cli_output_tests state_command_tests::cli_state_history -- --nocapture`
- `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- `cargo run --bin effigy -- docs check-paths docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/guides/017-json-output-contracts.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/batch-cards/611-add-state-history-read-only-command.md docs/roadmaps/g04/batch-cards/612-add-state-report-history-writes.md`
- `git diff --check`

## Next Task

Add timestamped history writes for plan/apply/capture reports.
