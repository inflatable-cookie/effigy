# 163 Implement Effigy Demo Browser Host Runtime Loop Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the last meaningful demo-browser host/runtime loop boundary so
`src/tui/demo_browser.rs` stops owning the browser run loop shape and becomes a
thin Effigy command adapter over `effigy-tui`.

## In Scope

- classify the remaining browser host/runtime loop contract explicitly
- move one more bounded browser loop or host-adapter slice into
  `crates/effigy-tui`
- leave direct Effigy command invocation and final process wiring explicit in
  the root crate
- reduce `src/tui/demo_browser.rs` again in a way that makes the remaining
  shell honest

## Out Of Scope

- full browser elimination from the root crate in one batch
- unrelated TUI cleanup outside the demo-browser seam
- release-lane execution

## Acceptance Criteria

- `crates/effigy-tui` owns the next host/runtime loop contract instead of the
  root file
- `src/tui/demo_browser.rs` shrinks or thins again in a meaningful way
- the remaining root browser shell is explicit enough for a post-batch boundary
  decision

## Validation

- bounded demo-browser TUI validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`164-decide-post-demo-browser-host-runtime-loop-boundary.md`
to decide whether the remaining browser shell is finally honest adapter work
or still needs one last bounded extraction batch.
