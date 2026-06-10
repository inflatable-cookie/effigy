# 2026-04-17 03:05:00 BST — Next Src Shell Cleanup Priority After CLI Help Pause Boundary Decision

## Summary

Completed `231` by choosing the next substantial `/src` cleanup priority from
the `g02.017` queue after pausing CLI help.

## Decision

Choose **process runtime extraction** (g02.017 queue job #4) next.

Target: a new `effigy-process` crate, not `effigy-exec`.

Reason:

- `src/process_manager/**` is a real cross-cutting subsystem (~726 lines:
  `ProcessSpec`, `ProcessEvent`, `ProcessSupervisor`, spawn/shutdown lifecycle,
  stdio streaming, signal handling, exit diagnostics)
- it is currently imported by 22+ call sites across `src/runner/**` and
  `src/tui/multiprocess/**`
- `g02.017` job #4 explicitly warns: "Only create a new process-runtime crate
  if `effigy-exec` would become artificially mixed"
- `effigy-exec` is container-execution routing (routing, cwd mapping, exec
  aliases, container detection, health checks). Merging generic process
  supervision there would force one crate to own two unrelated concerns
- the honest call is therefore a new `effigy-process` crate
- demo and docs runner shells remain under parallel-thread churn; process
  runtime is disjoint from both

## Next Task

Execute
[`232-implement-effigy-process-subsystem-extraction.md`](../../../specs/batch-cards/232-implement-effigy-process-subsystem-extraction.md)
to move `src/process_manager/**` into the new `effigy-process` crate.
