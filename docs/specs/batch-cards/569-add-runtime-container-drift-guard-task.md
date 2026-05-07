# 569 - Add Runtime Container Drift Guard Task

Lane: [`052-drift-guards-and-architecture-proof-matrix-strict-lane.md`](../052-drift-guards-and-architecture-proof-matrix-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add the first lightweight guard command for runtime/container architecture
drift.

## Scope

- add a focused task or script that runs the initial `rg` guard scans
- fail on forbidden direct calls outside allowed adapter modules
- document any current allowed matches clearly
- wire the guard into the local QA surface without touching CI workflows

## Non-Goals

- no `.github/workflows/` edits
- no full proof matrix implementation yet
- no broad runtime/container refactor
- no release work

## Exit Condition

This card is complete when a local guard command exists, it passes or reports
documented current drift, and the suppression/allowed-match policy is recorded
in the strict lane.

## Validation

- PASS: `bash scripts/check-runtime-container-drift.sh`
- PASS: `git diff --check`
- NOTE:
  `cargo run --bin effigy -- qa:architecture:runtime-container-drift` was
  attempted but blocked on an unrelated in-flight Cargo process holding the
  artifact lock. The direct task command body passed.

## Next Task

Card
[`570-add-runtime-container-proof-matrix-inventory.md`](./570-add-runtime-container-proof-matrix-inventory.md).
