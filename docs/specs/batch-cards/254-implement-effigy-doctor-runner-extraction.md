# 254 Implement Effigy-Doctor-Runner Extraction

Status: queued
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract `src/runner/doctor/**` (~4,547 lines, 65 files) into a new
workspace crate per card `253`'s decision. Follow the
`effigy-scan` / `effigy-builtin` extraction pattern: narrow
`DoctorError` → `RunnerError` boundary, port traits for any
residual runner reach-ins, no transitional shim in `src/lib.rs`.

Scope placeholders — filled in by the decide card:

- Crate name: `<decided in 253>`
- Error type: `<decided in 253>`
- Port surface: `<decided in 253>`
- Split shape: `<decided in 253>`
- Prerequisite cards: `<any opened by 253>`

## Context

Last major runner subsystem with a clean-ish boundary. After this
card lands:

- `src/runner/` holds only the core runtime (entrypoints, execute,
  deferral, locking, cache, error, manifest-glue, task listing /
  probe, release / container / bootstrap / script command shells).
- The only sizeable remaining seams are `execute/**` (847 LOC,
  runtime glue — not a candidate) and `tasks_listing/**` (1,466
  LOC, could shed its render layer but not urgent).

## In Scope

Populated after card `253` lands its decision. Shape will roughly
mirror `249` (scan extraction):

- Create the new workspace crate with `Cargo.toml` and `src/lib.rs`.
- Move `src/runner/doctor/**` contents into the new crate.
- Introduce `DoctorError` (or reuse `effigy-doctor`'s error).
- `impl From<DoctorError> for RunnerError` at the runner's edge.
- Port traits / direct deps per card `253`'s decision.
- Update caller files to import from the new crate directly
  (`src/runner/entrypoints/dispatch.rs`, any CLI consumers).
- Workspace `Cargo.toml` updates.

## Out Of Scope

- Anything `253` rules out.
- Other runner subsystem extractions (execute, tasks_listing).
- Changes to the existing `effigy-doctor` library crate's public
  surface beyond what the decision requires.

## Acceptance Criteria

- New workspace crate exists with the moved code.
- `src/runner/doctor/` directory gone.
- `From<DoctorError> for RunnerError` lives in
  `src/runner/error.rs`.
- No caller imports `crate::runner::doctor::*`.
- `cargo test --workspace`, `cargo fmt --all -- --check`, and
  `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err
  -A clippy::too_many_arguments -A clippy::type_complexity` all
  green.

## Next Task

Card `255` — flatten the test-harness prelude chain now that the
runner's internal layout is settled.
