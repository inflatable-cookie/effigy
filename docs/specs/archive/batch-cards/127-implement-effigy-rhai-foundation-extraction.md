# 127 Implement Effigy Rhai Foundation Extraction

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the first trustworthy Rhai host boundary out of `src/runner/script_command.rs`
so the scripting surface stops depending on one large runner-owned adapter.

## In Scope

- create the first real `effigy-rhai` crate boundary or equivalent workspace
  extraction slice
- move the reusable Rhai host/runtime contracts there
- reconnect the current runtime path without changing user-facing behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad Rhai feature widening
- release execution
- demo or env extraction in the same batch

## Acceptance Criteria

- more of the Rhai host surface no longer sits entirely in `runner`
- the scripting/runtime boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved Rhai contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `128-implement-effigy-demo-foundation-extraction.md` to move the next
largest still-interleaved domain cluster out of `runner`.
