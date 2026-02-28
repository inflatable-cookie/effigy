# TUI Core Extraction Checkpoint

Date: 2026-02-27

## Scope

Established a reusable internal TUI core module and migrated multiprocess TUI runtime to consume those shared primitives.

## Delivered

- Added `src/tui/core.rs` with shared primitives:
  - `InputMode`
  - `LogEntryKind`
  - `ProcessExitState`
  - `LogEntry`
  - `next_index` / `prev_index`
  - `toggle_follow_for_active`
- Updated multiprocess runtime modules to consume core primitives:
  - `src/tui/multiprocess/state.rs`
  - `src/tui/multiprocess/events.rs`
  - `src/tui/multiprocess/view_model.rs`
  - `src/tui/multiprocess/render.rs`
  - `src/tui/multiprocess/render/header.rs`
  - `src/tui/multiprocess/render/panes.rs`
  - `src/tui/multiprocess/render/footer.rs`
  - `src/tui/multiprocess/terminal_text.rs`
- Kept multiprocess-specific behavior (`OptionsAction`, process lifecycle) local to multiprocess modules.
- Updated architecture map: `docs/architecture/010-package-map.md`.

## Validation

- `cargo fmt --all`
- `cargo check -q`
- `cargo test -q`

All checks passed.

## Notes

This gives Effigy a clean seam for additional TUI implementations to share state models and common key-routing mechanics without depending on multiprocess internals.
