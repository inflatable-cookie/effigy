# 595 - Implement State Stack Manifest And Lineage Plan Foundation

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Add the first implementation foundation for state-stack manifest parsing,
validation, and lineage planning.

## Scope

- add dependency-light types for `effigy.state-stack.v1`
- parse the minimum stack and layer fields defined in contract `016`
- validate role ordering and environment policy decisions
- model a lineage plan/report that rolls up layer order and artifact-operation
  references
- include Example App-shaped fixtures for structure, baseline seed,
  legacy-import, dev-overlay, UAT capture, and full-capture roles

## Non-Goals

- no CLI parser surface unless needed for focused tests
- no app hook execution
- no Farmyard-specific migration behavior
- no durable persisted ledger
- no live rebase execution
- no release work

## Exit Condition

This card is complete when a focused crate/module can parse a state-stack
fixture, validate the first policy rules, and produce a deterministic lineage
plan/report without invoking app code.

## Closeout

- added `crates/effigy-state`
- added `effigy.state-stack.v1` manifest parsing
- added role ordering, environment policy, artifact-source, duplicate-key, and
  dependency validation
- added deterministic lineage planning and report shaping
- added Example App-shaped fixture coverage for structure, baseline seed,
  legacy-import, dev-overlay, UAT capture, and full-capture layers

## Validation

- PASS: `cargo test -p effigy-state`
- PASS: `git diff --check`

## Next Task

Card
[`596-add-state-stack-plan-command-surface.md`](./596-add-state-stack-plan-command-surface.md).
