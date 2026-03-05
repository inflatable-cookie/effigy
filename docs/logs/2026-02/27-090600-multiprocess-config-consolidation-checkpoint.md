# Multiprocess Config Consolidation Checkpoint

Date: 2026-02-27

## Scope

Finalized the multiprocess TUI cleanup pass by extracting runtime constants into a dedicated config module and documenting the tuning contract.

## Delivered

- Added `src/tui/multiprocess/config.rs` as the single source of truth for:
  - line buffer cap
  - event drain limits and timing
  - input poll cadence
  - vt parser dimensions/scrollback
  - shutdown grace timeout
- Updated multiprocess modules to consume config constants:
  - `src/tui/multiprocess/mod.rs`
  - `src/tui/multiprocess/events.rs`
  - `src/tui/multiprocess/lifecycle.rs`
  - `src/tui/multiprocess/terminal_text.rs`
- Updated package map entry in `docs/architecture/010-package-map.md`.
- Added architecture contract doc:
  - `docs/architecture/011-multiprocess-tui-config-contract.md`

## Validation

- `cargo fmt --all`
- `cargo check -q`
- `cargo test -q`

All checks passed.
