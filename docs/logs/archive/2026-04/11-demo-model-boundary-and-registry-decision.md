# 2026-04-11 - Demo Model Boundary And Registry Decision

Roadmap: `g02.003`

## Summary

Closed the first `g02.003` planning batch by fixing the demo object boundary
and registry posture.

Effigy demos are now defined as first-class repo-owned verification objects:

- registry root: `[demos]`
- per-demo identity: `[demos.<id>]`
- boundary: task-adjacent, not task-equivalent
- config posture: inline-first and later composition-compatible through
  `[manifest].include`

That means later batches no longer need to debate whether demos are “just
tasks,” whether the registry lives in another system, or whether demos need
their own include mechanism.

## Changes

- completed the `007` ready card
- recorded the demo registry and identity shape in the roadmap
- opened the next ready card for runner lifecycle and artifact semantics
- refreshed the currentness surfaces to point at the new active card

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Movement: baseline `demo proof was still a broad idea with no fixed object
  identity or registry boundary` -> current `demo proof now has a first-class
  Effigy-owned object boundary and registry shape`
- Remaining gap: `runner lifecycle, receipts/artifacts, coverage model, and
  browser contract are still planning work`

## Validation Performed

- command: `git diff --check`
  - result: passed
- command: `effigy qa:docs`
  - result: passed

## Next Task

Execute the active ready card in
`docs/roadmaps/g02/batch-cards/008-decide-demo-runner-lifecycle-and-artifact-boundaries.md`,
then leave the next move explicit as either coverage/gap modeling or browser/TUI
contract planning.
