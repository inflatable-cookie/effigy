# 2026-04-17 04:30:00 BST — Effigy UI Subsystem Extraction

## Summary

Moved the UI rendering subsystem out of the root crate and into a new
`effigy-ui` crate. `g02.017` queue job #6 shipped.

`src/ui/**` is gone. Renderer trait, PlainRenderer, theming, progress/spinner,
and table rendering now live at `crates/effigy-ui/src/**`. The crate depends
on `effigy-core` for widget data types (re-exported for caller ergonomics)
and owns the presentation dependencies (`anstream`, `anstyle`, `indicatif`,
`tabled`).

## Why This Batch

Per `g02.017` queue job #6, UI primitives were still root-crate owned across
47 caller files. Widget data types already lived in `effigy-core`, but the
rendering trait and concrete implementation did not.

Adding `anstream`, `anstyle`, `indicatif`, and `tabled` to `effigy-core`
would have pulled heavy presentation concerns into the pure-data core that
`effigy-manifest`, `effigy-bootstrap`, `effigy-process`, and `effigy-cli`
already depend on. A new `effigy-ui` crate is the honest boundary: it owns
presentation, depends on `effigy-core` for widgets, and lets `effigy-core`
keep its zero-deps posture.

## What Changed

- added `crates/effigy-ui/` with:
  - `lib.rs` — public surface: `Renderer`, `UiResult`, `UiError`,
    `SpinnerHandle`, `PlainRenderer`, `OutputMode`, plus widget re-exports
  - `renderer.rs` — `Renderer` trait and error types
  - `theme.rs` — `Theme` + output-mode detection
  - `progress.rs` — `NoopSpinnerHandle` and progress helpers
  - `table.rs` — table rendering helper using `tabled`
  - `plain_renderer/` — `PlainRenderer` implementation + 4 unit tests
- added `effigy-ui` to the workspace and as a root-crate dependency
- deleted `src/ui/**` entirely
- rewrote all 47 import sites from `use crate::ui::*` to
  `use effigy_ui::*` (direct imports, no re-export bridge retained)
- fixed two test files where a sed rewrite collapsed `PlainRenderer` into
  the `effigy_core::widgets::{...}` import list (PlainRenderer lives in
  `effigy_ui`, not `effigy_core`)
- kept widget data types in `effigy-core`; `effigy-ui` re-exports them from
  its public surface so callers can `use effigy_ui::{KeyValue, TableSpec,
  NoticeLevel, ...}` without reaching into `effigy-core` directly

## Churn Check

Real subsystem move. ~575 lines of UI primitives now live in a dedicated
crate with its own 4-test PlainRenderer harness. `effigy-core` keeps its
zero-deps posture; presentation deps are contained to `effigy-ui`.

## Vision Target Delta

- primary vision tags: `MAINT`, `CONTRACT`, `ROUTE`
- moved: UI rendering ownership is now explicit; presentation deps no longer
  leak into the pure core
- remaining open: post-`235` boundary decision for the UI subsystem

## Validation

- `cargo test -p effigy-ui` — 4/4 PlainRenderer tests green
- `cargo test` — full workspace green (11 test suites; root lib 746 → 742
  since the 4 PlainRenderer tests moved with the code)
- `cargo fmt --all -- --check` — clean
- `cargo run --bin effigy -- qa:docs` — passes
- `git diff --check` — clean

## Next Task

Execute
[`236-decide-post-effigy-ui-extraction-boundary.md`](../../../specs/batch-cards/236-decide-post-effigy-ui-extraction-boundary.md)
to classify the remaining UI-subsystem boundary honestly.
