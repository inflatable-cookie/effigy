# Demo Browser Foundation Slice Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `019`

## Summary

The first honest demo browser slice is now fixed.

Effigy's new self-hosted demos proved two things:

- `browser-proof-report` shows that the shipped list, grouping, inspect, and
  artifact-link surface is already coherent enough for browser-style discovery
  and detail inspection.
- `lifecycle-window` shows that the real remaining ergonomic gap is
  single-surface interaction for `run`, `inspect`, and `stop`, not more CLI
  query features.

That means the next implementation slice should not be terminal emulation or
generic runtime expansion. It should be a bounded demo browser with list/detail
navigation and in-browser action dispatch.

## Decision

The next browser/TUI batch should implement:

- a sidebar or grouped list of demos
- a detail pane for the selected demo
- refresh and bounded `run`, `stop`, `rerun` actions from the same surface
- state rendering based on the shipped demo registry, query, inspect, active
  attempt, latest receipt, and artifact-path contracts

The next browser/TUI batch should not implement:

- generic runtime cancellation
- embedded terminal streaming
- rich artifact rendering
- multi-attempt history or queueing

## Why This Slice Is Honest

- It addresses the concrete two-terminal pain exposed by `lifecycle-window`
  without pretending Effigy already has broader cancellable runtime handles.
- It reuses the shipped runner contract instead of inventing a UI-local demo
  model.
- It keeps the first interactive client small enough to ship before live log or
  rich artifact concerns widen the scope.

## Evidence

- `cargo run --bin effigy -- demo run browser-proof-report`
- `cargo run --bin effigy -- demo inspect browser-proof-report`
- `cargo run --bin effigy -- demo run lifecycle-window`
- `cargo run --bin effigy -- demo inspect lifecycle-window`
- `cargo run --bin effigy -- demo stop lifecycle-window`
- generated artifacts under `.effigy/demo/artifacts/`
- generated receipts under `.effigy/demo/receipts/`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `browser/TUI boundary still abstract after CLI runner work`
  to `first bounded browser foundation slice fixed against live self-hosted
  proof demos`
- Remains open: actual browser/TUI implementation, live log visibility,
  artifact-opening affordances, broader runtime stoppability

## Next Task

Implement `020-implement-demo-browser-list-detail-foundation.md` as the first
interactive demo browser slice.
