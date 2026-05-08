# 138 Implement Effigy Doctor Foundation Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the first trustworthy doctor-domain cluster into its own workspace crate
so manifest schema and doctor reference policy no longer depend entirely on a
runner-local module tree.

## In Scope

- add the first `effigy-doctor` workspace crate
- move the next reusable doctor ownership there
- reconnect the current runner path without changing user-facing behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad doctor surface extraction in one batch
- vault-provider rollout work
- release closure

## Acceptance Criteria

- more of the doctor surface no longer sits entirely in `runner`
- the doctor boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved doctor contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`139-decide-post-doctor-foundation-extraction-boundary.md`](./139-decide-post-doctor-foundation-extraction-boundary.md)
to classify the remaining doctor shell before modularization jumps again.
