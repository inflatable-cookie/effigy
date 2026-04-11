# 2026-04-11 - Demo Runner Lifecycle And Artifact Boundary Decision

Roadmap: `g02.003`

## Summary

Closed the second `g02.003` planning batch by fixing the first runner contract
for demos.

Effigy now has a bounded runner direction for demo proof:

- command family: `effigy demo`
- operator actions: list, inspect, run, stop, rerun
- lifecycle states: `planned`, `ready`, `running`, `passed`, `failed`,
  `broken`, `missing`
- receipts are runner-normalized verification records
- artifacts are repo-produced outputs referenced by receipts, not rich formats
  owned by the runner

That means later batches no longer need to debate whether demo execution lives
under generic task routing, what the minimum status model is, or whether the
runner owns artifact rendering itself.

## Changes

- completed the `008` ready card
- recorded the demo runner lifecycle contract in the roadmap
- opened the next ready card for coverage and gap modeling
- refreshed the currentness surfaces to point at the new active card

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Movement: baseline `demo proof had an object model but no settled execution
  lifecycle or receipt boundary` -> current `demo proof now has a dedicated
  runner surface, bounded lifecycle actions, and a clear receipt/artifact
  division of responsibility`
- Remaining gap: `coverage/gap semantics, browser contract, and pilot
  reconciliation are still planning work`

## Validation Performed

- command: `git diff --check`
  - result: passed
- command: `effigy qa:docs`
  - result: passed

## Next Task

Execute the active ready card in
`docs/specs/batch-cards/009-decide-demo-coverage-and-gap-model.md`, then leave
the next move explicit as either browser/TUI contract shaping or pilot
reconciliation against Signal.
