# 137 Decide Post-Env Foundation Extraction Boundary

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining env / varlock shell still justifies another
`effigy-env` extraction batch or whether modularization should move to the
next largest interleaved cluster.

## In Scope

- assess the remaining env / varlock weight outside `effigy-env`
- distinguish honest adapter work from still-reusable env-domain logic
- leave the next modularization move explicit

## Out Of Scope

- implementing another extraction slice in the same batch
- vault-provider rollout work
- release closure

## Acceptance Criteria

- the remaining env shell is classified honestly
- the next modularization move is explicit
- `g02.010` currentness stays trustworthy

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`138-implement-effigy-doctor-foundation-extraction.md`](./138-implement-effigy-doctor-foundation-extraction.md)
to move the first reusable doctor cluster out of `runner`.
