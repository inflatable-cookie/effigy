# 610 - Add State Lineage History Lookup Design Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Define how Effigy should find previous state plan/apply/capture reports without
turning the report directory into a premature database.

## Scope

- define report naming and lookup semantics for state history
- decide how plan, apply, and capture reports relate to lineage ids
- preserve append-only/audit-friendly behavior where possible
- keep manual report files readable and easy to inspect
- identify the smallest implementation slice

## Non-Goals

- no database-backed ledger
- no automatic conflict detection
- no sync daemon
- no release work

## Exit Condition

This card is complete when the contract says how operators and future commands
can locate prior state reports for a stack and the next implementation card is
bounded.

## Validation

- `cargo run --bin effigy -- docs check-paths docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/batch-cards/610-add-state-lineage-history-lookup-design-boundary.md docs/roadmaps/g04/batch-cards/611-add-state-history-read-only-command.md`
- `git diff --check`

## Next Task

Implement the smallest read-only `state history` command.
