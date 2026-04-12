# Demo Post-Concurrent-Runner Session Projection Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: `067-decide-demo-post-concurrent-runner-session-projection-boundary`

## Summary

Chose one more runner-owned fidelity slice next: flattened terminal
interaction for concurrent-runner-backed demos. Browser follow-up stays
deferred, and the lane does not pause yet.

## Decision

- do not take a browser/client follow-up next
- do not pause terminal/runtime work yet
- the next bounded slice is runner-owned concurrent-runner terminal
  interaction projection:
  - input forwarding
  - resize semantics
  - still flattened behind one demo-scoped session contract

## Why This Is The Right Boundary

- concurrent-runner-backed demos can now project active output and stop
  semantics, but they still fall short of the interaction contract already
  available for run-backed demos
- taking browser work next would force UI decisions before the richer backend
  is honest enough
- pausing now would leave the concurrent-runner path materially weaker than
  the runner contract already advertises as the product boundary
- input/resize projection is the smallest useful next step that keeps the
  no-nested-TUI rule intact

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `DEMO`
- Moved: `output-and-stop-only concurrent projection -> explicit next runner-owned interaction slice`
- Remaining: implement bounded concurrent-runner input/resize projection through the demo session contract

## Next Task

- Execute `068-implement-demo-concurrent-runner-terminal-interaction-projection.md`

