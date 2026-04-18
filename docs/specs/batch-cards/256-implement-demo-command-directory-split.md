# 256 Implement Demo Command Directory Split

Status: landed
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Split `src/runner/demo_command.rs` into a real module directory so the
root runner no longer carries one 2k+ line file that mixes dispatch,
rendering, execution orchestration, process/runtime control, and OS shell
behavior.

## Context

The post-`246` audit showed `demo_command.rs` is now the largest mixed
root-crate shell seam left in `/src`. It already depends on extracted
`effigy-demo`, `effigy-managed`, `effigy-process`, and TUI/browser
surfaces, but the runner-side adapter is still concentrated in one file.

The goal here is not another crate move. The goal is to leave an honest
runner shell with smaller owned modules and clearer local boundaries.

## In Scope

- Convert `src/runner/demo_command.rs` into `src/runner/demo_command/`.
- Keep one small `mod.rs` or `dispatch.rs` entrypoint that owns
  `run_demo(args: DemoArgs)`.
- Split the current file into bounded local modules along the already
  visible seams:
  - dispatch / entrypoint routing
  - query and record loading
  - render/list/inspect/history output
  - task-backed execution
  - run-backed execution
  - runtime/process helpers
- Keep behavior and user-facing output unchanged.
- Keep the public runner surface unchanged for callers.

## Out Of Scope

- New crate extraction.
- Changes to `effigy-demo` API shape unless a tiny adapter helper is
  required to complete the split cleanly.
- Browser/TUI follow-up work outside the demo runner command.
- Cleanup of `release_command.rs` or `container_command.rs`.

## Acceptance Criteria

- `src/runner/demo_command.rs` no longer exists as a monolithic file.
- The replacement directory has a small entrypoint plus focused local
  modules with one primary responsibility each.
- No user-facing output, JSON schema, or runtime behavior changes.
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`

## Next Task

Card `257` — split `src/runner/release_command.rs` the same way once the
demo runner shell is no longer the biggest root-crate hotspot.
