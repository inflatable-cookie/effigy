# g05.014 - Area-Local Test Builder Cleanup

Status: Complete
Depends on: `g05.011`, `g05.012`, `g05.013`
Contract: [`030-low-risk-deduplication-contract.md`](../../contracts/030-low-risk-deduplication-contract.md)

## Goal

Remove the remaining high-confidence fixture duplication with private,
area-local builders instead of one global harness.

## Evidence

- duplicate-block scan still reports repeated bootstrap, release, container, and
  CLI fixture setup
- the latest audit called out repeated Rhai `ScriptContext` setup, repeated
  bootstrap/release fixture flows, and repeated CLI manifest/file writers
- the active dedup contract already says fixture cleanup should stay private and
  near the tests that use it

## Scope

- add private builders or local support modules where repeated setup obscures
  the behavior under test
- target Rhai script context setup, bootstrap/release fixture setup, and
  repeated CLI manifest/file writers first
- keep domain behavior tests in domain crates and runner tests adaptation-focused

## Non-Goals

- no public test-support crate
- no snapshot churn without value
- no abstraction that hides the behavior under test

## Acceptance Criteria

- duplicate-block findings drop in the targeted test areas
- tests remain readable about the behavior they prove
- ownership stays local to each area instead of drifting into a mega harness

## Suggested Validation

- targeted test subsets for each touched area
- `effigy scan duplicate-blocks --json`
- `git diff --check`

## Next Task

No next task inside this roadmap. Residual high-duplication seams are deferred
by ownership in `734`.
