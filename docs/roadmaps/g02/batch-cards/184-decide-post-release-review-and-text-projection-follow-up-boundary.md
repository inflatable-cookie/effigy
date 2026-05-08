# 184 Decide Post-release Review And Text Projection Follow-up Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the release seam is finally honest enough to stop blocking
`g02.007`, now that the release review/menu/text-projection layer is crate-owned
and the interactive release contract has been revalidated.

## In Scope

- assess the remaining `src/runner/release_command.rs` shell honestly
- decide whether the release seam can pause
- leave one explicit next move:
  - either release-lane resumption
  - or one more bounded modularization batch
- update lane state and currentness surfaces honestly

## Out Of Scope

- release execution
- unrelated modularization outside the remaining release shell
- speculative extraction beyond what the remaining code actually justifies

## Acceptance Criteria

- the remaining release shell is described concretely, not vaguely
- the next move is explicit and trustworthy
- `continue` resolves through the boundary decision instead of stale `183`
  pointers

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`185-decide-next-src-shell-cleanup-priority-after-release-boundary.md`](./185-decide-next-src-shell-cleanup-priority-after-release-boundary.md)
to choose the next `/src` shell seam, since the release seam can pause but
`g02.010` still cannot.
