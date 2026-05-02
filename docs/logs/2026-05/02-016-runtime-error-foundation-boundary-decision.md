# 02 016 Runtime Error Foundation Boundary Decision

Date: 2026-05-02
Roadmap: `g03.016`
Spec: `docs/specs/030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`
Batch: `345`

## Decision

Keep `g03.016` open.

## Why

`344` proved the lane is real, but it did not remove `task_invocation` as the
dominant failure shape across the runtime/container core.

The remaining highest-signal stringly seam is now:

- container surface resolution in `exec_command/surface.rs`
- container-surface selection failures such as:
  - missing `[containers]`
  - no `context = "dev"` container
  - multiple dev-context containers
  - missing named container
  - container not running
- runner-side translation of container policy failures that still flatten into
  generic invocation strings

That is a bounded follow-up slice with clear product value and direct
category-level testability.

## Next Lane Step

Execute `346`.
