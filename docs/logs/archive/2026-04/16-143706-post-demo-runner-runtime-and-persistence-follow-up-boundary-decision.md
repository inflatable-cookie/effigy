# Post Demo Runner Runtime And Persistence Follow-up Boundary Decision

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`166` keeps the demo runner seam open.

`165` removed a real persistence cluster, but
[src/runner/demo_command/mod.rs](../../../../src/runner/demo_command/mod.rs) still
owns one more reusable demo-domain layer around records and projections. That
is still larger and more reusable than the next obvious `/src` seams, so the
right move is one more bounded `effigy-demo` extraction batch instead of
jumping to another file.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `post-165 demo runner boundary undecided` -> current
  `demo runner seam kept open for one more record/projection extraction`
- Remaining gap: `shared demo record and projection ownership in demo_command`

## Evidence

- `src/runner/demo_command.rs`: `4292` lines
- still-owned reusable demo-domain cluster:
  - `DemoRecord`
  - `DemoActionAvailability`
  - `DemoGroup`
  - query/history/list projection helpers
- more shell-shaped remainder after that:
  - text rendering
  - process/runtime launch control
  - command entry orchestration

## Decision

Do not move off the demo runner seam yet.

The next ready card is:

- `167-implement-effigy-demo-record-and-projection-follow-up-extraction.md`

## Next Task

Execute
`167-implement-effigy-demo-record-and-projection-follow-up-extraction.md`.
