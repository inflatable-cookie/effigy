# 2026-02-27 Runner and TUI Modularization Checkpoint

Date: 2026-02-27
Owner: betterthanclay / codex
Related roadmap: 004-dev-process-manager-tui, 005-unified-testing-orchestration

## Scope
- Capture the current modularization state after splitting the previous large runner/TUI files.
- Record final module boundaries, design intent, and remaining cleanup opportunities.

## Changes
- Split TUI runtime into reusable module namespace:
  - `src/tui/mod.rs`
  - `src/tui/multiprocess/mod.rs`
  - `src/tui/multiprocess/render.rs`
  - `src/tui/multiprocess/terminal_text.rs`
- Split runner monolith into focused modules:
  - `src/runner/mod.rs` (orchestration)
  - `src/runner/model.rs` (shared runner domain model + constants)
  - `src/runner/manifest.rs` (manifest schema + serde normalization)
  - `src/runner/catalog.rs` (catalog discovery + selector logic)
  - `src/runner/builtin.rs` (built-in task routing + built-in test orchestration)
  - `src/runner/managed.rs` (managed process planning and runtime)
  - `src/runner/deferral.rs` (deferral selection/execution)
  - `src/runner/render.rs` (runner report/trace rendering)
  - `src/runner/util.rs` (runner utility helpers)
- Updated architecture package map to reflect new boundaries:
  - `docs/architecture/010-package-map.md`

## Boundary Intent
- `runner/mod.rs` should remain a composition and entrypoint layer only.
- `runner/model.rs` should own shared types/constants to avoid cross-module circular leakage.
- `runner/manifest.rs` should isolate TOML schema and serde behavior from execution logic.
- `runner/render.rs` should isolate formatting concerns from execution and routing code.
- `tui/multiprocess/*` should be reusable for non-dev task UIs in future roadmap phases.

## Validation
- command: `cargo check -q`
  - result: pass
- command: `cargo test -q`
  - result: pass
  - notes: full suite green after module extractions and rewiring.

## Risks / Follow-ups
- `runner/mod.rs` still contains mixed orchestration + some execution path logic; further extraction to `runner/execute.rs` could tighten boundaries.
- Shared constants in `runner/model.rs` are broad; a later split into `runner/constants.rs` may improve discoverability.
- Tests currently bind through `runner/mod.rs`; consider targeted module-level tests for `builtin`, `deferral`, and `managed` to localize failures.
- `runner/manifest.rs` currently combines schema and behavior helpers; future split to `manifest/schema.rs` and `manifest/normalize.rs` is possible if complexity grows.

## Next
- Continue with a final thin-layer pass on `runner/mod.rs`: move remaining execution helpers into `runner/execute.rs`, keep only command dispatch and shared wiring, then rerun full suite.
