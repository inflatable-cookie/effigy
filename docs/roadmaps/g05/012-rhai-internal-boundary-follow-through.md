# g05.012 - Rhai Internal Boundary Follow-Through

Status: Complete
Depends on: `g05.010`

## Goal

Reduce `crates/effigy-rhai/src/lib.rs` to a facade and wiring layer so the Rhai
crate keeps its current public shape without one internal choke point.

## Evidence

- `crates/effigy-rhai/src/lib.rs` is still 1659 lines and warning-level
- the file mixes runtime context, secrets, process execution, streaming/PTTY,
  search helpers, HTTP helpers, and registration scaffolding
- the earlier host API split improved crate shape but left too much internal
  ownership in `lib.rs`

## Scope

- keep `lib.rs` as a facade and module-wiring surface
- extract secrets, process execution, streaming, and helper conversions into
  focused internal modules
- preserve the current public Rhai host surface and callback behavior

## Non-Goals

- no new public crate
- no Rhai language-surface expansion
- no callback contract redesign

## Acceptance Criteria

- `lib.rs` is materially smaller and easier to reason about
- the internal module layout matches durable concern boundaries
- Rhai public behavior and tests remain stable

## Suggested Validation

- `cargo test -p effigy-rhai`
- targeted runner tests that exercise Rhai callbacks
- `effigy scan god-files --json`

## Next Task

Open a card for the first internal extraction slice: secrets and process support
that currently sit alongside facade wiring in `lib.rs`.
