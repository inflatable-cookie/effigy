# 162 Decide Post Demo Browser Runtime Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/tui/demo_browser.rs` shell is now honest
runtime/process adapter work or whether one more bounded extraction batch is
still justified before the browser seam can pause.

## In Scope

- inspect the remaining root browser shell after batches `147` through `161`
- classify what is still reusable domain/TUI API versus true runtime shell
- decide whether another demo-browser extraction batch is warranted
- update lane state and next-task surfaces honestly

## Out Of Scope

- implementation work beyond the decision itself
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- the remaining browser shell is described concretely
- the next move is explicit:
  - either one more ready extraction card
  - or an honest browser pause boundary inside `g02.010`
- docs currentness reflects the real state

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

One more bounded browser extraction batch is still justified.

The remaining `src/tui/demo_browser.rs` shell is smaller, but it is not yet a
clean adapter:

- top-level browser run loop ownership still sits in the root file
- Effigy command invocation still sits behind one local `invoke_demo_json(...)`
  bridge
- runtime execution, shutdown, refresh, and terminal resize still route
  through one root-owned host adapter
- the file still carries meaningful browser integration tests alongside that
  shell

That is still enough reusable browser/runtime boundary to justify one more
explicit extraction batch before calling this seam honest shell-only work.

## Next Task

Execute
`163-implement-effigy-demo-browser-host-runtime-loop-extraction.md`
to extract the remaining host/runtime loop contract and leave the root file as
the final Effigy command adapter shell.
