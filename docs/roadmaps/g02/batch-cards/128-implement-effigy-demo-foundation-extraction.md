# 128 Implement Effigy Demo Foundation Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the first trustworthy demo-domain boundary out of `src/runner/demo_command.rs`
so the largest remaining product surface stops depending on one large
runner-owned adapter.

## In Scope

- create the first real `effigy-demo` crate boundary or equivalent workspace
  extraction slice
- move the reusable demo-domain contracts there
- reconnect the current runtime path without changing user-facing behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad demo feature widening
- release execution
- env or docs-policy extraction in the same batch

## Acceptance Criteria

- more of the demo-domain surface no longer sits entirely in `runner`
- the demo/runtime boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved demo contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the next modularization batch using the widened demo boundary, now that
receipt/history/path ownership no longer sits entirely in `runner`.
