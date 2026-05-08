# 145 Implement Effigy TUI Foundation Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the first bounded TUI/runtime surface into a real workspace crate so
the remaining `src/tui` weight stops being treated as unnamed shell residue.

## In Scope

- classify the first bounded reusable TUI/runtime contracts
- add one workspace crate for that slice
- move the extracted TUI/runtime contracts out of `src/tui`
- reconnect the root crate through thin adapters

## Out Of Scope

- full demo-browser decomposition in one batch
- release-lane execution
- unrelated runner cleanup

## Acceptance Criteria

- one real TUI/runtime crate is added and used
- `src/tui` meaningfully shrinks or thins around that boundary
- the next remaining shell seam is explicit

## Validation

- bounded TUI/runtime validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `146-implement-effigy-multiprocess-tui-foundation-extraction.md` to
keep shrinking the remaining `src/tui` runtime shell.
