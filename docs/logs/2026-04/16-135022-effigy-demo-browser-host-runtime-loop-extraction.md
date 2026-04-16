# Effigy Demo Browser Host Runtime Loop Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`163` moved the browser app shell itself into `effigy-tui`.

`crates/effigy-tui/src/demo_browser.rs` now owns `DemoBrowserApp`, the browser
run loop, the host/runtime loop helpers, and the generic invoke-json boundary
used by that app shell. The root file [src/tui/demo_browser.rs](/Users/tom/Dev/projects/effigy/src/tui/demo_browser.rs)
is now reduced to the direct Effigy command bridge plus browser tests.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `root crate still owns demo-browser app shell` -> current
  `browser app shell extracted into effigy-tui; root production shell reduced
  to launch plus command bridge`
- Remaining gap: `post-browser boundary decision and next /src seam selection`

## Evidence

- `src/tui/demo_browser.rs`: `1387` -> `1132` lines
- `crates/effigy-tui/src/demo_browser.rs`: `3633` -> `3934` lines

## Validation Performed

- `cargo fmt --all`
- `cargo test demo_browser --lib`
- `cargo test -p effigy-tui`

## Next Task

Execute
`164-decide-post-demo-browser-host-runtime-loop-boundary.md`.
