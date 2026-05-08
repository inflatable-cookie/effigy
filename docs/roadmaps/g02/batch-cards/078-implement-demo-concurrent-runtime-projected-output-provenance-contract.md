# 078 Implement Demo Concurrent Runtime Projected Output Provenance Contract

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add one more bounded runner-owned truth layer for projected concurrent-runner
demos: what output provenance survives when multiple managed processes are
flattened behind one demo-owned terminal/session surface.

## In Scope

- add demo-owned projected-output provenance facts for concurrent-runner demos
  that stay on the flattened path
- expose those facts through inspect and active terminal/session payloads
- keep the contract bounded enough that browser consumers can stay honest
  without inventing process-manager UI
- preserve the no-nested-TUI rule

## Out Of Scope

- multi-process browser panes or tabs
- embedded concurrent TUI
- generic process-manager controls
- browser chrome refreshes
- desktop-client work

## Acceptance Criteria

- projected concurrent demo payloads say whether merged output is:
  - unlabeled
  - source-attributed
  - or otherwise flattened without per-line provenance
- the next runner truth is demo-scoped rather than generic process inventory
- the lane remains anchored in one active ready card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch turns into browser redesign
- the contract starts requiring nested TUI launch
- the slice becomes multi-process control work instead of bounded provenance

## Next Task

Execute [`079-decide-demo-post-projected-output-provenance-boundary.md`](./079-decide-demo-post-projected-output-provenance-boundary.md)
to decide whether projected concurrent demos now earn one bounded browser
follow-up or should stay runner-side longer.
