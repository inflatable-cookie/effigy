# 2026-04-16 19:30:00 BST — Next Src Shell Cleanup Priority After Bootstrap Boundary Decision

## Summary

Chose distribution as the next `/src` shell-cleanup seam.

Bootstrap is now paused on an honest adapter boundary, so the lane needed to
pick the next still-reusable root-crate cluster instead of grinding further on
bootstrap.

## Decision

The next ready batch is distribution extraction, not another bootstrap card and
not a move into the parallel container-design work.

## Why

- demo and release are already paused on previously-recorded honest shell
  boundaries
- bootstrap is now down to callback wiring and projection rendering
- `src/runner/distribution_command.rs` is still a bounded product surface with
  coherent reusable execution/artifact logic still living in `runner`
- container remains a live neighboring area because parallel design work is
  laying down new crates and docs there, so distribution is the cleaner next
  seam for this thread

## Vision Target Delta

The modularization lane stays focused on real domain boundaries instead of
churn. Distribution is now the next place where `/src` still owns reusable
product logic that should sit behind a crate boundary.

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`189-implement-effigy-distribution-execution-and-artifact-follow-up-extraction.md`](../../../specs/batch-cards/189-implement-effigy-distribution-execution-and-artifact-follow-up-extraction.md).
