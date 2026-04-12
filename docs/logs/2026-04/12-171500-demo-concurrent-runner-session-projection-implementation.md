# Demo Concurrent-Runner Session Projection Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Card: `066-implement-demo-concurrent-runner-session-projection`

## Summary

Projected concurrent-runner-backed demos through the existing demo session
contract so demo-scoped inspect, active-session, and stop surfaces can report
honest concurrent-runner facts without nested TUI launch.

## What Shipped

- detect concurrent-runner-backed demo task entrypoints through catalog/task
  resolution instead of treating them as generic task-backed demos
- execute managed concurrent demo tasks through a flattened demo-owned runtime
  path that captures stdout/stderr logs, active attempt state, and receipt
  history
- report `runtime_backend = concurrent-runner` at the demo, active-attempt,
  and active-terminal-session layers with flattened projection semantics
- allow `demo stop` to stop concurrent-runner-backed active demos through the
  runner-owned active-attempt contract
- added CLI regression coverage for inactive classification, active inspect
  projection, and stop/termination behavior

## Validation

- `cargo test concurrent_runner -- --nocapture`
- full batch validation still required before closeout commit

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `DEMO`
- Moved: `backend identity only -> one richer concurrent-runner backend projected through the demo contract`
- Remaining: decide the next bounded slice after concurrent-runner session projection lands

