# 136 Implement Effigy Env Foundation Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the env-schema / varlock foundation into its own workspace crate so the
secret-resolution and env-contract surface no longer depends on one binary-local
module tree.

## In Scope

- add the first `effigy-env` workspace crate
- move the next trustworthy env-schema / varlock ownership there
- reconnect the current runtime path without changing user-facing behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad vault-provider rollout work
- doctor extraction in the same batch
- release closure

## Acceptance Criteria

- more of the env-schema / varlock surface no longer sits entirely in the root
  crate
- the env boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved env contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`137-decide-post-env-foundation-extraction-boundary.md`](./137-decide-post-env-foundation-extraction-boundary.md)
to classify the remaining env / varlock shell before modularization jumps to
doctor, release, or another env slice.
