# 2026-04-17 04:45:00 BST — Post Effigy UI Extraction Boundary Decision

## Summary

UI rendering pauses cleanly.

After `235`, no `crate::ui`, `mod ui`, or `pub mod ui` reference remains
anywhere in the root crate. All UI-rendering call sites (47 files) use
`use effigy_ui::*` directly — no wrapper, adapter, or bridge residue. The
`src/ui/` directory is gone. Widget types still live in `effigy-core` and
are re-exported from `effigy-ui` for caller ergonomics.

## Why This Decision

Further extraction would mean one of two things:

- pulling `crossterm` terminal probing (currently owned by `cli_help.rs` and
  TUI subsystems) into `effigy-ui` — that would mix renderer primitives with
  terminal-capability detection, neither of which belongs to the other
- forcing widget data types out of `effigy-core` and into `effigy-ui` — that
  would break the established pattern (widget types are pure data that
  non-rendering crates already depend on)

Both are fake completeness work. `effigy-ui` currently owns exactly the
right layer: the `Renderer` trait, the `PlainRenderer` concrete impl, theming,
progress/spinner primitives, and table rendering. Pause.

## Decision

- pause UI rendering on the current boundary
- keep `effigy-ui` as the owner of renderer primitives and presentation deps
  (`anstream`, `anstyle`, `indicatif`, `tabled`)
- keep `effigy-core` as the owner of widget data types with zero deps
- move the active lane to `g02.017` queue job #8 (post-subsystem runner
  adapter cleanup survey)

## Churn Check

Real boundary. `235` moved ~575 lines of UI primitives plus 4 tests; the
remaining `use effigy_ui::*` imports across 47 caller files are the minimum
viable rendering surface for a root crate that does not own rendering.

## Vision Target Delta

- primary vision tags: `MAINT`, `CONTRACT`, `ROUTE`
- moved: UI rendering now paused on a clean cross-cutting crate boundary,
  disjoint from widget data types in `effigy-core`
- remaining open: `g02.017` queue job #8 — rerun the `/src` churn check now
  that both cross-cutting subsystems (process, UI) have moved

## Validation

- `cargo test` — full workspace green (11 suites)
- `cargo run --bin effigy -- qa:docs` — passes
- `git diff --check` — clean

## Next Task

Execute
[`237-decide-post-subsystem-runner-adapter-cleanup-survey.md`](../../../specs/batch-cards/237-decide-post-subsystem-runner-adapter-cleanup-survey.md)
to rerun the `/src` churn check after the process + UI subsystem moves.
