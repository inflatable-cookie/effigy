# 2026-04-11 - Demo Coverage And Gap Model Decision

Roadmap: `g02.003`

## Summary

Closed the third `g02.003` planning batch by fixing how proof coverage and gaps
are modeled.

Effigy now has an explicit coverage direction for demo proof:

- all known proof obligations remain visible in the `[demos.<id>]` registry
- `planned` and `missing` are explicit gap states, not silence inferred from
  absent files
- `broken` is an existing proof surface that currently cannot be trusted
- `stale` is a freshness overlay on existing proof, not a new base lifecycle
  state
- demos carry explicit coverage claims through `covers = ["area.key"]`

That means later batches no longer need to debate whether proof gaps are
inferred, how stale proof differs from broken proof, or what minimum data the
browser needs to show proof coverage honestly.

## Changes

- completed the `009` ready card
- recorded the coverage and gap model in the roadmap
- opened the next ready card for browser/TUI contract shaping
- refreshed the currentness surfaces to point at the new active card

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Movement: baseline `demo proof had an object and runner model, but no settled
  way to express proof gaps or freshness` -> current `demo proof now has an
  explicit coverage and gap model that the browser can depend on`
- Remaining gap: `browser/TUI contract, pilot reconciliation, and
  implementation planning are still ahead`

## Validation Performed

- command: `git diff --check`
  - result: passed
- command: `effigy qa:docs`
  - result: passed

## Next Task

Execute the active ready card in
`docs/specs/batch-cards/010-decide-demo-browser-and-tui-contract.md`, then
leave the next move explicit as either pilot reconciliation against Signal or
the first bounded implementation-planning lane.
