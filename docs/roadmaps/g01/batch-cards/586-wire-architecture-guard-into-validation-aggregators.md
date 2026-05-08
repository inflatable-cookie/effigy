# 586 - Wire Architecture Guard Into Validation Aggregators

Lane: [`058-architecture-guard-integration-strict-lane.md`](../058-architecture-guard-integration-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Run runtime/container architecture guards from normal QA aggregators.

## Scope

- add `qa:architecture` to `qa:gates`
- add `qa:architecture` to `qa:ci`
- add `qa:architecture` to `prepush:ci`
- document the guard suppression policy in the runtime operation pipeline
  contract
- keep day-to-day `qa` unchanged unless the task graph already routes through
  gates

## Non-Goals

- no new large-file guard in this card
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when common validation paths include the existing drift
guard and suppression expectations are documented.

## Validation

- `cargo run --bin effigy -- qa:architecture`
- `cargo run --bin effigy -- qa:gates --plan`
- `cargo run --bin effigy -- prepush:ci --plan`
- `git diff --check`

Note: `qa:architecture` and `qa:gates --plan` passed. `prepush:ci --plan`
executes the full task graph in this repo and reached unrelated existing
full-suite failures; the guard wiring itself ran before those failures.

## Next Task

Start
[`587-split-effigy-container-ops-module-owners.md`](./587-split-effigy-container-ops-module-owners.md).
