# Post Demo Record And Projection Follow-up Boundary Decision

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`168` keeps the demo runner seam open.

`167` removed the shared record/projection layer cleanly, but
[src/runner/demo_command/mod.rs](../../../src/runner/demo_command/mod.rs)
still owns a reusable demo execution/runtime cluster. That cluster is still
larger and more domain-shaped than the next obvious `/src` seams, so the right
move is one more bounded `effigy-demo` extraction batch instead of shifting to
another file.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `post-167 demo runner boundary undecided` -> current
  `demo runner seam kept open for execution/runtime follow-up extraction`
- Remaining gap: `shared demo execution/log/runtime ownership in demo_command`

## Evidence

- [src/runner/demo_command/mod.rs](../../../src/runner/demo_command/mod.rs):
  `3964` lines
- remaining reusable demo-domain cluster:
  - `DemoExecutionAttempt`
  - `DemoLogPaths`
  - run-backed launch and output capture helpers
  - concurrent-runner runtime state and projection helpers
  - receipt/history/log persistence shaping around executed attempts
- more shell-shaped remainder after that:
  - command entry wiring
  - text/json render wiring
  - final runner adapter behavior

## Decision

Do not move off the demo runner seam yet.

The next ready card is:

- `169-implement-effigy-demo-execution-runtime-and-attempt-follow-up-extraction.md`

## Next Task

Execute
`169-implement-effigy-demo-execution-runtime-and-attempt-follow-up-extraction.md`.
