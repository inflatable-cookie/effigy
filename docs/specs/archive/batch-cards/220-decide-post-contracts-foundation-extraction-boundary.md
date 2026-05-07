# 220 Decide Post Contracts Foundation Extraction Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the new `effigy-contracts` workspace boundary is sufficient to
pause the contracts seam, or whether `src/runner/contracts_command.rs` still
owns one more real contracts-domain slice.

## In Scope

- assess the remaining `contracts_command.rs` shell after `219`
- classify what is now crate-owned versus still runner-owned
- decide whether contracts should pause or continue for one more bounded batch
- promote that boundary into the active lane/currentness surfaces

## Out Of Scope

- demo/docs/container cleanup
- release execution
- speculative new crate work beyond the contracts seam

## Acceptance Criteria

- the post-`219` contracts boundary is explicit
- the next move is clear:
  - either pause contracts and choose the next `/src` seam
  - or open one more bounded contracts follow-up card
- active lane/currentness surfaces point at the decided next move

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`221-decide-next-src-shell-cleanup-priority-after-contracts-boundary.md`](./221-decide-next-src-shell-cleanup-priority-after-contracts-boundary.md)
to choose the next substantial shell cleanup target after the contracts pause
boundary.
