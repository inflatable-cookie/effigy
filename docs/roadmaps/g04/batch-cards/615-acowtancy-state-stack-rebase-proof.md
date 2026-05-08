# 615 - Acowtancy State Stack Rebase Proof

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Exercise the new Effigy state-stack surface against the Acowtancy migration
workflow and identify the smallest adapter seam still missing.

## Scope

- inspect the Acowtancy migration code and docs from the consumer side
- draft or wire a minimal `[state]` stack declaration that maps current phases
  onto Effigy layers
- prove the capture task context can drive one repo-owned capture/export task
- keep Acowtancy transform, media, and conflict logic in Acowtancy
- report any Effigy framework gaps as follow-up cards rather than smuggling app
  semantics into Effigy

## Non-Goals

- no production migration execution
- no old-site sync daemon
- no full Acowtancy migration rewrite
- no release work

## Exit Condition

This card is complete when the Acowtancy repo has a concrete proof path for
rebasing at least one existing migration/capture phase onto Effigy's state-stack
surface, or a short blocking-gap list explains why not.

## Validation

- consumer-side dry-run or plan command where feasible
- focused tests only if Acowtancy code is changed
- Effigy docs/card update if new framework gaps are discovered
- `git diff --check` in any touched repo

## Next Task

Close the first state-stack release slice and decide whether the next release
should hold at this boundary.
