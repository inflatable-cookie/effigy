# 232 Implement Effigy Process Subsystem Extraction

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`, `g02.017` (queue job #4)
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the cross-cutting process supervision subsystem out of the root crate
and into a new `effigy-process` crate. This is `g02.017` queue job #4.

## In Scope

- create `crates/effigy-process` with:
  - `ProcessSpec`, `ProcessEvent`, `ProcessEventKind`
  - `ProcessSupervisor` + its control surfaces (spawn, input, terminate,
    restart, shutdown, event pump)
  - `ProcessManagerError`
  - `ShutdownProgress`
- move `src/process_manager.rs` and `src/process_manager/**` into
  `crates/effigy-process/src/`
- add `effigy-process` to the workspace and depend on it from the root crate
- update all callers across `src/runner/**` and `src/tui/multiprocess/**` to
  `use effigy_process::*`
- update `runner::error::RunnerError` `From<ProcessManagerError>` path

## Out Of Scope

- merging with `effigy-exec` (container routing crate) — the roadmap warns
  against artificial mixing and process supervision is a disjoint concern
- turning the subsystem into an async runtime (tokio-style) — scope is a
  clean relocate
- demo/docs/container parallel cleanup

## Acceptance Criteria

- `crates/effigy-process` exists and is used by the root crate
- `src/process_manager.rs` + `src/process_manager/` no longer exist
- all previous call sites work unchanged via `use effigy_process::*`
- `cargo test` green across the workspace

## Validation

- `cargo test`
- `cargo fmt --all -- --check`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`233-decide-post-effigy-process-extraction-boundary.md`](./233-decide-post-effigy-process-extraction-boundary.md)
to classify the remaining process-supervision boundary honestly.
