# Demo Concurrent-Runner Terminal Interaction Projection Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Card: `068-implement-demo-concurrent-runner-terminal-interaction-projection`

## Summary

Projected bounded input and resize interaction for concurrent-runner-backed
demos through the demo session contract so detached browser and CLI consumers
can use the same demo-owned handoff surface without nested TUI launch.

## What Shipped

- concurrent-runner-backed detached demo sessions now expose runner-owned
  stdin and resize handoff paths
- active attempt and active terminal/session payloads now report honest input
  forwarding and resize availability for eligible flattened concurrent demos
- the managed runtime loop now polls the demo-owned input handoff and forwards
  appended input into one flattened target process when the concurrent demo
  resolves to a single process
- `demo input` and `demo resize` now work for concurrent-runner-backed
  detached demo sessions through the same demo contract used by browser
  consumers
- added CLI regression coverage for active inspect, detached input, detached
  resize, and stop behavior on concurrent-runner-backed demos

## Validation

- `cargo test concurrent_runner -- --nocapture`
- full batch validation still required before closeout commit

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `DEMO`
- Moved: `concurrent projection output-only -> bounded input and resize interaction through the demo session contract`
- Remaining: decide the next bounded slice after concurrent-runner interaction projection lands

