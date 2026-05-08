# 208 Decide Post Container Runner Shell Follow Up Cleanup Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/container_command.rs` shell is now
honest enough to pause after the `207` container execution/session extraction.

## In Scope

- assess the remaining runner-local container shell after `207`
- compare what still lives in the runner against the crate-owned
  `effigy-containers` APIs
- decide whether:
  - the container seam can pause on an honest shell boundary
  - or one more bounded container shell cleanup batch is still justified
- update lane state and currentness surfaces honestly

## Out Of Scope

- new container-design roadmap work
- release-lane execution
- speculative multi-seam reprioritization before the container boundary is
  judged

## Acceptance Criteria

- the remaining container shell is described concretely
- the pause-or-continue decision is explicit
- the next move is one ready card, not an ambiguous list

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`209-decide-next-src-shell-cleanup-priority-after-container-boundary.md`](./209-decide-next-src-shell-cleanup-priority-after-container-boundary.md)
to choose the next honest `/src` shell cleanup priority after the container
seam pause.
