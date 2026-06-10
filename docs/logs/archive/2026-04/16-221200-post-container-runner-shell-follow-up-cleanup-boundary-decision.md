# 2026-04-16 22:12:00 BST — Post Container Runner Shell Follow Up Cleanup Boundary Decision

## Summary

The container seam can pause.

## Why This Decision

After `207`, `src/runner/container_command.rs` is down to `790` lines and the
remaining runner-local weight is mostly:
- CLI command entry and payload shaping
- repo/path resolution and final JSON/text rendering
- inherited child-process spawning
- signal/process-group shutdown handling
- final runner error mapping over crate-owned container APIs

What used to be the container-domain seam is now crate-owned in
`effigy-containers`:
- Colima running/start checks
- compose execution and capture helpers
- shutdown execution
- attached session mode and process planning
- stream overview and closeout rendering

That leaves process-shell and adapter behavior, not another honest
`effigy-containers` extraction target.

## Decision

Pause the container seam and move to a broader `/src` priority decision.

## Churn Check

Keeping the lane on container any longer would be fake completeness work. The
remaining shell there is narrower than the still-open seams elsewhere in the
repo.

## Vision Target Delta

- primary vision tags: `MAINT`, `OPERATE`
- moved: container runner shell from mixed domain ownership to an honest
  adapter/process boundary
- remaining open: choose the next `/src` seam that still holds real
  modularization value

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`209-decide-next-src-shell-cleanup-priority-after-container-boundary.md`](../../../specs/batch-cards/209-decide-next-src-shell-cleanup-priority-after-container-boundary.md)
to choose the next honest `/src` shell cleanup priority after the container
seam pause.
