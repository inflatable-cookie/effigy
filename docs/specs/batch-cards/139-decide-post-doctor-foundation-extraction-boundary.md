# 139 Decide Post-Doctor Foundation Extraction Boundary

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining doctor shell still justifies another
`effigy-doctor` extraction batch or whether modularization should move to the
next largest interleaved cluster.

## In Scope

- assess the remaining doctor weight outside `effigy-doctor`
- distinguish honest adapter/orchestration work from still-reusable
  doctor-domain logic
- leave the next modularization move explicit

## Out Of Scope

- implementing another extraction slice in the same batch
- release closure
- vault-provider rollout work

## Acceptance Criteria

- the remaining doctor shell is classified honestly
- the next modularization move is explicit
- `g02.010` currentness stays trustworthy

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`140-implement-effigy-doctor-report-and-projection-extraction.md`](./140-implement-effigy-doctor-report-and-projection-extraction.md)
to move the reusable doctor report/result cluster out of `runner`.
