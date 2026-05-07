# 135 Decide Post-Docs-Policy QA Check Extraction Boundary

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining docs-owned shell still justifies another
`effigy-docs-policy` extraction batch or whether docs-policy is now clean
enough for modularization to move to the next largest interleaved cluster.

## In Scope

- assess the remaining docs weight in `src/runner/docs_command.rs`
- distinguish honest CLI adapter work from still-reusable docs-policy logic
- leave the next ready batch explicit

## Out Of Scope

- implementing another extraction slice in the same batch
- doctor extraction unless the decision explicitly promotes it next
- release closure

## Acceptance Criteria

- the remaining docs shell is classified honestly
- the next modularization move is explicit
- `g02.010` currentness stays trustworthy

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`136-implement-effigy-env-foundation-extraction.md`](./136-implement-effigy-env-foundation-extraction.md)
to move the env-schema / varlock foundation into its own workspace crate.
