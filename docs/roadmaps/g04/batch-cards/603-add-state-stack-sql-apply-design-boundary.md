# 603 - Add State Stack SQL Apply Design Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Design the SQL apply adapter boundary before executing SQL payload layers from
state stacks.

## Scope

- define how `apply_mode = "sql"` maps onto existing data/container database
  surfaces
- decide whether SQL apply should execute directly or delegate through
  repo-owned tasks
- define safety gates, target selection, and report shape
- preserve artifact staging and task execution behavior from earlier cards
- document the first implementation boundary after design

## Non-Goals

- no SQL execution in this card
- no media mutation
- no capture adapter
- no app-specific migration logic
- no record-level reconciliation
- no release work

## Exit Condition

This card is complete when the SQL apply adapter contract is explicit enough to
implement without accidentally coupling Effigy to one app database shape.

## Validation

- PASS: `cargo run --bin effigy -- docs check-paths docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/roadmaps/g04/batch-cards/603-add-state-stack-sql-apply-design-boundary.md docs/roadmaps/g04/batch-cards/604-add-state-stack-sql-import-adapter-boundary.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md`
- PASS: `git diff --check`

## Next Task

Start
[`604-add-state-stack-sql-import-adapter-boundary.md`](./604-add-state-stack-sql-import-adapter-boundary.md).
