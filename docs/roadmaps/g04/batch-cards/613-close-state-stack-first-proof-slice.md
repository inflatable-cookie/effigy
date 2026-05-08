# 613 - Close State Stack First Proof Slice

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Close the first state-stack proof slice as a coherent release candidate surface
before adding more capture or reconciliation semantics.

## Scope

- review command, contract, and JSON docs for drift
- run a broader validation pass over the state-stack implementation
- identify any remaining first-slice defects that block the next Effigy release
- leave a clear next boundary for capture context hardening or Acowtancy adapter
  proof work

## Non-Goals

- no new state-stack command family
- no conflict detection
- no app-specific Acowtancy transform implementation
- no release work

## Exit Condition

This card is complete when the implemented state-stack slice is documented,
validated, and either ready to hold as the next release boundary or has a short
blocking-fix list.

## Validation

- focused state-stack CLI tests
- relevant crate/unit tests
- JSON contract checks
- docs path checks for touched planning docs
- `git diff --check`

## Next Task

Harden the state capture task context contract before asking Acowtancy to rebase
onto the new surface.
