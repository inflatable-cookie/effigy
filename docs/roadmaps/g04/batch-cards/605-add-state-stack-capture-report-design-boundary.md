# 605 - Add State Stack Capture Report Design Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Define the first capture report boundary for state stacks without implementing
database/media capture execution yet.

## Scope

- define what `effigy state capture` should report
- model capture sources, destinations, environment, and stack lineage links
- keep UAT-authored overlay capture separate from full-system capture
- define how captured artifacts should reference existing OCI/local artifact
  reports
- document which parts remain repo-owned hooks
- identify the narrowest next implementation card

## Non-Goals

- no capture execution
- no data diff engine
- no media rewrite semantics
- no conflict detection or reconciliation
- no release work

## Exit Condition

This card is complete when the contract describes a capture report shape and
the next implementation boundary is small enough to execute without smuggling
Acowtancy-specific logic into Effigy.

## Validation

- `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- `cargo run --bin effigy -- docs check-paths docs/contracts/016-state-stack-and-layered-seed-framework-contract.md docs/guides/026-json-payload-examples.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/batch-cards/605-add-state-stack-capture-report-design-boundary.md docs/roadmaps/g04/batch-cards/606-add-plan-only-state-capture-command-surface.md`
- `git diff --check`

## Next Task

Implement plan-only `state capture` so operators can see the report before any
capture execution exists.
