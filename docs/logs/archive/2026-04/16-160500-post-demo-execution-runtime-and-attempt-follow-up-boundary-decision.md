# Post Demo Execution Runtime And Attempt Follow-up Boundary Decision

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`170` keeps the demo runner seam open.

`169` removed the shared attempt/log execution layer cleanly, but
[src/runner/demo_command/mod.rs](../../../../src/runner/demo_command/mod.rs)
still owns a reusable runtime-control cluster. That cluster is still larger and
more domain-shaped than the next obvious `/src` seams, so the right move is
one more bounded `effigy-demo` extraction batch instead of shifting to another
file.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `post-169 demo runner boundary undecided` -> current
  `demo runner seam kept open for runtime-control follow-up extraction`
- Remaining gap: `shared demo runtime/process control ownership in demo_command`

## Evidence

- [src/runner/demo_command/mod.rs](../../../../src/runner/demo_command/mod.rs):
  `3804` lines
- remaining reusable demo-domain cluster:
  - concurrent-runner runtime state and event-loop handling
  - run-backed launch mode and PTY/stream process shaping
  - output capture / input handoff helpers
  - runtime backend classification and projected-process helpers
- more shell-shaped remainder after that:
  - command entry wiring
  - text/json render wiring
  - final runner adapter behavior

## Decision

Do not move off the demo runner seam yet.

The next ready card is:

- `171-implement-effigy-demo-runtime-control-and-process-follow-up-extraction.md`

## Next Task

Execute
`171-implement-effigy-demo-runtime-control-and-process-follow-up-extraction.md`.
