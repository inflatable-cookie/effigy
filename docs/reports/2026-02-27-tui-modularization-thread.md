# TUI Modularization Thread

Date: 2026-02-27
Owner: Platform
Related roadmap: 004, 006

## Scope

Consolidated the TUI modularization effort into one thread with architecture outcomes and end-to-end smoke evidence.

## Threaded checkpoints

- `2026-02-27-runner-and-tui-modularization-checkpoint.md`
- `2026-02-27-tui-modularization-phase-2-checkpoint.md`
- `2026-02-27-tui-core-extraction-checkpoint.md`
- `2026-02-27-multiprocess-config-consolidation-checkpoint.md`

## Final structure snapshot

- Shared TUI core primitives:
  - `src/tui/core.rs`
- Multiprocess runtime split:
  - `src/tui/multiprocess/mod.rs` (orchestration)
  - `src/tui/multiprocess/state.rs`
  - `src/tui/multiprocess/events.rs`
  - `src/tui/multiprocess/view_model.rs`
  - `src/tui/multiprocess/lifecycle.rs`
  - `src/tui/multiprocess/config.rs`
- Render split:
  - `src/tui/multiprocess/render.rs` (orchestration)
  - `src/tui/multiprocess/render/header.rs`
  - `src/tui/multiprocess/render/panes.rs`
  - `src/tui/multiprocess/render/footer.rs`
  - `src/tui/multiprocess/render/help_overlay.rs`

## Smoke matrix

### Non-interactive CLI checks

- command: `cargo run --quiet --bin effigy -- tasks`
  - result: passes; renders header and built-in task catalog.
- command: `cargo run --quiet --bin effigy -- help`
  - result: passes; renders command help panel.
- command: `cargo run --quiet --bin effigy -- test --plan`
  - result: passes; detects `cargo-nextest` and shows fallback chain.

### Interactive TUI checks (PTY)

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- dev`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy`
  - result: starts multiprocess TUI with expected tabs (`dairy/dev`, `cream/dev`, `farmyard/api`, `farmyard/jobs`, `shell`), processes stream output, Ctrl+C exits cleanly, per-process summary printed.
- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test --tui`
  - cwd: `/Users/betterthanclay/Dev/projects/effigy`
  - result: starts TUI test runner, displays test output stream, Ctrl+C exits cleanly, summary + test results rendered.

## Validation

- command: `cargo fmt --all`
  - result: pass
- command: `cargo check -q`
  - result: pass
- command: `cargo test -q`
  - result: pass (118 tests)

## Risks / Follow-ups

- ANSI-heavy PTY output still depends on terminal emulator behavior and parser limits under extreme throughput.
- Additional manual smoke on the largest monorepo remains recommended before final release cut.

## Next

- Prepare and stage a single modularization commit grouped by subsystem:
  - runner modularization
  - TUI modularization/core/config
  - architecture/report docs
