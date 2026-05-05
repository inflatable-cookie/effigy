# 397 - Decide Runtime Container Cleanup Closeout Boundary

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Decide whether `g03.033` is ready to close or needs one more bounded cleanup
slice.

## Scope

- review the `g03.033` exit condition
- inventory remaining direct cwd/backend/task-dispatch drift
- distinguish compatibility wrappers from brittle caller-local logic
- either create a final cleanup card or close the lane
- no implementation changes in this decision card

## Exit Condition

This card is complete when the lane either points at one final implementation
card or has an explicit closeout card.

## Decision

Close `g03.033` with a closeout card.

The remaining drift is not caller-local runtime/container glue:

- `crates/effigy-containers/src/compose.rs` remains the legacy compatibility
  wrapper over `ContainerManager`
- `crates/effigy-containers/src/lib.rs` keeps Colima-specific policy guards for
  temp-root and nerdctl mount-label-budget validation
- one `std::env::current_dir()` remains in an exec-command unit-test temp-root
  helper

Runner production command code no longer carries the main brittle shapes this
lane targeted:

- direct cwd/root probing in migrated command callers
- direct Docker/Colima/nerdctl command construction
- direct compose backend selection
- duplicate runtime-prep execution-surface labels

## Next Boundary

Do not split the remaining large container files in this lane just to satisfy a
line-count target. The next roadmap, `g03.034`, should prove the new surfaces
against DecodeLabs and Underlay fixtures. Contract cleanup and any public
compatibility decisions belong in `g03.035`.

## Next Task

Close `g03.033`.
