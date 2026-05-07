# 129 Implement Demo Browser And Projection Foundation Extraction

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the next trustworthy demo-domain slice out of `src/runner/demo_command.rs`
and `src/tui/demo_browser.rs` so browser/list/inspect projection logic stops
depending on one large runner-owned adapter.

## In Scope

- widen `effigy-demo` around reusable demo projection and browser-facing model
  ownership
- move the next trustworthy shared demo contracts there
- reconnect the current CLI and TUI runtime paths without changing user-facing
  behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad demo feature widening
- release execution
- env or docs-policy extraction in the same batch

## Acceptance Criteria

- more of the demo browser/projection surface no longer sits entirely in
  `runner` and `tui`
- the demo browser/runtime boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved demo projection/browser contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the next modularization batch using the widened demo boundary, now that
shared browser/list/inspect payload ownership no longer sits only in `runner`
and `tui`.
