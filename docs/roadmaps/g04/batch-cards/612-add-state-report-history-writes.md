# 612 - Add State Report History Writes

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Write state plan/apply/capture reports into the file-based history layout
defined by the contract, without introducing a database-backed ledger.

## Scope

- preserve the existing `plan.json` compatibility write
- add latest pointers for plan, apply, and capture reports
- add timestamped `history/*.json` report files
- include written paths in reports where appropriate
- keep manual deletion safe

## Non-Goals

- no retention or pruning
- no database-backed ledger
- no conflict detection
- no release work

## Exit Condition

This card is complete when state report-producing commands can write latest and
timestamped history files that `state history` can read.

## Validation

- focused CLI tests for written report paths and history lookup
- JSON contract checks
- `git diff --check`

## Next Task

Close the first state-stack proof slice with a documentation and validation
pass before adding deeper capture semantics.
