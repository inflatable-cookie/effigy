# 173 Implement Effigy Demo Managed Runtime And Backend Follow-up Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the remaining reusable demo runtime-control layer from
`src/runner/demo_command.rs` into `effigy-demo` so the runner keeps only the
raw command/process shell.

## In Scope

- move managed demo runtime state into `effigy-demo`
- move concurrent-runner runtime backend classification and projected-process
  helpers into `effigy-demo`
- move stop/attach capability helpers that belong to the shared demo runtime
  contract into `effigy-demo`
- rewire `src/runner/demo_command.rs` to adapt the extracted crate API
- update lane state and currentness surfaces honestly

## Out Of Scope

- release-lane execution
- unrelated `src/runner/` cleanup outside the demo seam
- browser/TUI work that already paused cleanly

## Acceptance Criteria

- `effigy-demo` owns the reusable managed-runtime/backend layer
- `src/runner/demo_command.rs` is materially smaller and reads more clearly as:
  - command entry/render wiring
  - raw process launch/execution shell
  - final runner adapter behavior
- docs currentness reflects the new active batch honestly

## Validation

- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`
- `cargo test --test cli_output_tests demo`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Outcome

This batch is now shipped.

`effigy-demo` owns the reusable managed-runtime/backend layer:

- managed concurrent-runner runtime state
- non-zero-exit rendering
- backend/projection shaping for concurrent-runner demos
- input-target selection and browser-live-attach capability truth

`src/runner/demo_command.rs` is smaller again and now reads more clearly as:

- command entry/render wiring
- raw process launch and supervisor orchestration
- final runner adapter behavior

## Next Task

Execute
[`174-decide-post-demo-managed-runtime-and-backend-follow-up-boundary.md`](./174-decide-post-demo-managed-runtime-and-backend-follow-up-boundary.md).
