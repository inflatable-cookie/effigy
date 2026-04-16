# 2026-04-16 19:15:00 BST — Post Bootstrap Foundation Extraction Boundary Decision

## Summary

Paused the bootstrap seam.

The first bootstrap extraction removed the reusable bootstrap-domain layer from
`src/runner/bootstrap_command.rs`. What remains there is runner-local callback
bridging plus text/json rendering, which is now honest shell work rather than
another hidden domain seam.

## Decision

Bootstrap does not get another immediate follow-up batch.

The next move is a broader `/src` shell-cleanup prioritization decision so the
lane can choose the next real seam instead of grinding bootstrap into a fake
micro-extraction.

## Why

- bootstrap request and execution contracts already moved into
  `effigy-bootstrap`
- bootstrap git sync and child-bootstrap orchestration already moved there too
- the remaining runner code is mostly:
  - manifest/task callback wiring
  - runner error mapping
  - text/json projection
- those are adapter concerns, not the next obvious reusable crate boundary

## Vision Target Delta

The modularization lane is closer to the intended thin-shell posture. Bootstrap
is no longer an outlier root-crate product surface, and the next cleanup choice
can be made against the remaining `/src` seams more honestly.

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`188-decide-next-src-shell-cleanup-priority-after-bootstrap-boundary.md`](../../specs/batch-cards/188-decide-next-src-shell-cleanup-priority-after-bootstrap-boundary.md)
to choose the next bounded modularization seam.
