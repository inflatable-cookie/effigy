# 395 - Decide Remaining Backend Branching Cleanup Boundary

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Choose the next bounded cleanup for backend-specific branching after container
inspection command shape moved behind `ContainerManager`.

## Scope

- inventory remaining direct backend checks in runner and container crates
- distinguish compatibility-layer branching from caller-local branching
- choose one narrow implementation card, or close this part of `g03.033` if
  only compatibility-layer branching remains
- no implementation changes in this decision card

## Exit Condition

This card is complete when the lane has a clear next card or an explicit
closeout decision for backend-branching cleanup.

## Decision

Move Colima start runtime selection behind `ContainerManager`.

Reasoning:

- runner command code no longer contains direct backend branching
- `crates/effigy-containers/src/exec.rs` no longer branches directly for runtime
  inspection commands
- the remaining production `resolve_compose_backend()` calls are compatibility
  surfaces in `compose.rs`, Colima startup, and Colima-specific policy guards
- Colima startup currently asks the legacy compose backend enum whether it
  should emit `--runtime docker` or `--runtime containerd`
- that decision is backend detection, not Colima command assembly

## Deferred

- Colima-specific policy guards in `crates/effigy-containers/src/lib.rs`; these
  remain compatibility-layer validation until the contract cleanup milestone
  decides whether to promote a backend capability API for them.
- `compose.rs` itself; it remains the old compatibility wrapper over
  `ContainerManager` until all consumers can stop naming `ComposeBackend`.

## Next Task

Implement card `396`: move Colima start runtime selection behind
`ContainerManager`.
