# g05.019 - Schema Shape Regression Proof And Closeout

Status: Complete
Depends on: `g05.017`, `g05.018`

## Goal

Close the schema-shape consolidation suite with proof that the duplicated owner
surfaces are reduced and the current supported TOML syntax still behaves the
same.

## Scope

- rerun focused manifest, bundle, bootstrap, and state schema tests
- document any intentionally retained duplicate schema owners and why they stay
- refresh planning/currentness surfaces if this suite becomes the active `g05`
  runway

## Non-Goals

- no new syntax work
- no adjacent runtime refactors disguised as validation

## Acceptance Criteria

- focused regression proof is recorded
- retained duplicate schema owners, if any, are explicit and justified
- the suite closes without stale planning pointers

## Completed

- Reran focused manifest, bootstrap, and state schema tests.
- Reran the full `effigy-manifest` crate single-threaded to avoid known
  test-global home override interference.
- Documented retained task-like owners in `g05.018`.
- Refreshed the g05 front doors so schema-shape convergence is closed and the
  reusable-core hardening suite is the next queued tranche.

## Validation

- `cargo test -p effigy-manifest single_task_object_without_array_wrapper`
- `cargo test -p effigy-manifest bootstrap_run_accepts_compact_inline_task_run_in`
- `cargo test --lib compact_inline_task_run_in`
- `cargo test -p effigy-manifest -- --test-threads=1`
- `cargo fmt --all -- --check`
- `effigy docs check paths docs/roadmaps`

## Next Task

Schema-shape convergence is complete. The next queued tranche is
`g05.020` through `g05.027`, starting with deploy-provider contract hardening
unless the user chooses a different priority.
