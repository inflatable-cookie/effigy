# 235 Implement Effigy UI Subsystem Extraction

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`, `g02.017` (queue job #6)
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the UI rendering subsystem out of the root crate and into a new
`effigy-ui` crate so UI primitives (Renderer trait, PlainRenderer, Theme,
table/progress helpers) stop being root-crate owned. This is `g02.017`
queue job #6.

## In Scope

- create `crates/effigy-ui` with:
  - `Renderer` trait, `UiError`, `UiResult`, `SpinnerHandle`
  - `Theme` + output-mode detection
  - `NoopSpinnerHandle` + progress helpers
  - table rendering helper
  - `PlainRenderer` concrete implementation and its unit tests
- depend on `effigy-core` for widget data types (`NoticeLevel`, `TableSpec`,
  `KeyValue`, `MessageBlock`, `StepState`, `SummaryCounts`) — no duplicate
  types, no new ownership
- own presentation dependencies (`anstream`, `anstyle`, `indicatif`, `tabled`)
  at the crate boundary so they stop reaching the root crate
- update all `use crate::ui::*` call sites (47 files) to
  `use effigy_ui::*`
- delete `src/ui/**` from the root crate
- keep `crossterm` usage local to callers that genuinely need terminal
  probing (`src/cli_help.rs`, TUI subsystems)

## Out Of Scope

- folding UI primitives into `effigy-core` (the roadmap warns against mixing
  pure data with presentation, and `effigy-core` has zero deps today)
- moving `crossterm`-backed terminal probing (separate concern from renderer
  primitives)
- demo/docs/container parallel cleanup
- TUI crate restructuring beyond import path updates

## Acceptance Criteria

- `crates/effigy-ui` exists and is used by the root crate
- `src/ui/**` no longer exists in the root crate
- all 47 caller files work unchanged via `use effigy_ui::*`
- widget types still live in `effigy-core`; `effigy-ui` re-exports them from
  its public surface for caller ergonomics
- `cargo test` green across the workspace
- PlainRenderer unit tests migrated with the code

## Validation

- `cargo test`
- `cargo fmt --all -- --check`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`236-decide-post-effigy-ui-extraction-boundary.md`](./236-decide-post-effigy-ui-extraction-boundary.md)
to classify the post-`235` UI-subsystem boundary honestly.
