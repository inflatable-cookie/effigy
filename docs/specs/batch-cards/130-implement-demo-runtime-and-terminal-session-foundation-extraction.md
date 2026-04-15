# 130 Implement Demo Runtime And Terminal Session Foundation Extraction

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the next trustworthy demo-domain slice out of `src/runner/demo_command.rs`
and `src/tui/demo_browser.rs` so runtime-backend, active-attempt, and terminal
session orchestration stop depending on one large runner-owned adapter.

## In Scope

- widen `effigy-demo` around reusable demo runtime/session contracts
- move the next trustworthy shared demo runtime and terminal-session model
  ownership there
- reconnect the current CLI and TUI runtime paths without changing user-facing
  behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad demo feature widening
- release execution
- env or docs-policy extraction in the same batch

## Acceptance Criteria

- more of the demo runtime/session surface no longer sits entirely in `runner`
  and `tui`
- the demo runtime/browser boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved demo runtime/session contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`131-decide-post-demo-runtime-and-terminal-session-extraction-boundary.md`](./131-decide-post-demo-runtime-and-terminal-session-extraction-boundary.md)
to decide whether the remaining demo shell is still extraction-worthy or
whether modularization should move on to the next domain cluster.
