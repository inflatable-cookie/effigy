# 218 Next Src Shell Cleanup Priority After Release Final Pause Boundary Decision

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `next-src-shell-cleanup-priority-after-release-final-pause-boundary-decision`

## Summary

Chose the contracts surface as the next active `/src` cleanup priority.

The release seam is now paused, and the largest remaining runner shell is demo,
but that demo seam is already being worked in parallel. The contracts surface is
the next substantial disjoint target and still looks like one more justified new
crate candidate.

## Decision

Move the active lane to `src/runner/contracts_command.rs`.

Why contracts wins now:

- `src/runner/demo_command.rs` is still larger, but it is already active in the
  parallel cleanup queue
- the release seam is now paused on an honest runner-shell boundary
- `src/runner/contracts_command.rs` is still product-shaped and coherent enough
  to justify a dedicated crate boundary
- this is the cleanest next substantial seam that avoids write-set conflict

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `ROUTE`
- Movement: baseline `release seam just paused; next shell priority still open`
  -> current `contracts selected as the next active shell-cleanup seam`
- Remaining gap: `demo, docs, widgets, and bootstrap still remain in the queued
  cleanup program`

## Validation Performed

- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- the parallel demo thread may still change the size/shape of the overall
  cleanup queue, but it does not invalidate contracts as the cleanest disjoint
  active seam
- the docs pass still emits unrelated demo warnings from that parallel work

## Next Task

- Execute `219-implement-effigy-contracts-foundation-extraction.md`.
