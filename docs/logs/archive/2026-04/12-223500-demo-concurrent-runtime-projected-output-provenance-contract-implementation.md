# Demo Concurrent Runtime Projected Output Provenance Contract

Status: complete
Created: 2026-04-12
Roadmap: g02.003
Batch: 078-implement-demo-concurrent-runtime-projected-output-provenance-contract

## Summary
- Added runner-owned projected output provenance facts for flattened
  concurrent-runner demos.
- Exposed the new truth through inspect and active terminal/session payloads.

## Changes
- added `runtime_backend.projected_output_provenance` to demo detail,
  active attempt, and active terminal/session surfaces
- current projected provenance now reports:
  - `single-source` for one-process concurrent projections
  - `flattened-unlabeled` for multi-process merged projections
- persisted active attempt records now carry projected-output provenance truth
- browser JSON consumers now deserialize the new field
- extended CLI tests for inactive and active concurrent-runner projections

## Vision Target Delta
- Primary tags: `demo`, `runner`, `concurrent-runtime`, `projection`
- Movement: baseline `process names and merge truth only` -> current `output provenance truth added to projected concurrent demo contract`
- Remaining gap: clients still need a boundary call on whether this new truth
  earns a bounded browser follow-up

## Validation Performed
- command: `cargo test`
  - result: passed
- command: `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
  - result: passed
- command: `cargo run --bin effigy -- qa`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- projected concurrent demos still flatten multiple processes into one demo
  surface; richer browser consumption still needs a boundary decision

## Next Task
- Execute `079-decide-demo-post-projected-output-provenance-boundary.md`.
