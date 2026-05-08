# 125 Decide Post Release Persistence Extraction Boundary

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the new `effigy-release` ownership around prepared-state
persistence, fingerprint drift, and mutation application is enough to move the
lane toward a modularization boundary decision.

## In Scope

- assess what still sits materially inside `release_command.rs`
- judge whether the remaining work is adapter-side runtime shell behavior or
  still domain extraction debt
- leave one explicit next ready card or boundary decision

## Out Of Scope

- resuming `g02.007`
- executing the release
- broad new release feature work

## Acceptance Criteria

- the remaining release extraction debt is stated clearly
- the lane either points at a final modularization decision or one more honest
  release extraction batch
- the next move is explicit

## Validation

- docs/state surfaces updated honestly
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `126-decide-modularization-boundary-before-v0-3-release-resumption.md`
to judge whether `g02.010` can now pause honestly and clear `g02.007` to
resume.
