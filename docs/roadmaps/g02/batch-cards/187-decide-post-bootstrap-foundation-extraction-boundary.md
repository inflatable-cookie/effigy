# 187 Decide Post-Bootstrap Foundation Extraction Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining bootstrap shell in
`src/runner/bootstrap_command.rs` is now honest enough to pause or whether one
more bounded bootstrap follow-up is still justified before the lane shifts to
the next `/src` seam.

## In Scope

- assess what moved into `effigy-bootstrap`
- assess what still sits in `src/runner/bootstrap_command.rs`
- decide whether the remainder is shell/adapter work or still contains another
  crate-worthy bootstrap-domain cluster
- update lane state and currentness surfaces honestly

## Out Of Scope

- implementing another bootstrap slice in the same batch
- release closure
- reprioritizing away from `g02.010`

## Acceptance Criteria

- the remaining bootstrap shell is classified explicitly
- the next move is either:
  - a bounded bootstrap follow-up card
  - or the next broader `/src` seam decision
- no false pause boundary is introduced

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`188-decide-next-src-shell-cleanup-priority-after-bootstrap-boundary.md`](./188-decide-next-src-shell-cleanup-priority-after-bootstrap-boundary.md).
