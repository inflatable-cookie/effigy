# 398 - Close Runtime Container Caller Migration And Cleanup

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Close `g03.033` and hand off to the dependability proof matrix.

## Scope

- mark `g03.033` complete
- mark strict lane `039` complete
- update `g03` front doors to make `g03.034` the next queued work
- record closeout notes and validation
- do not change code

## Exit Condition

This card is complete when the active roadmap/spec front doors no longer point
at `g03.033`, and the next task is explicit.

## Closeout

`g03.033` is closed.

Delivered cleanup:

- runner cwd/root callers now consume the active runtime context helper
- runtime prep no longer carries duplicate execution-surface labels
- container inspection and Colima runtime selection route through
  `ContainerManager`
- runner production code has no direct Docker/Colima/nerdctl process
  construction
- remaining backend resolver calls are documented as lower-level
  compatibility-layer validation or wrappers

## Validation

- `rg "std::env::current_dir\(\)|resolve_compose_backend\(\)|Command::new\(\"(docker|colima|nerdctl)\"|ExecutionSurfaceKind|TaskExecutionRequestBuilder" src/runner crates/effigy-containers/src crates/effigy-container-manager/src -n`
- validation from cards `392`, `394`, and `396`

## Next Task

Open `g03.034`: dependability proof matrix for DecodeLabs and Underlay shapes.
