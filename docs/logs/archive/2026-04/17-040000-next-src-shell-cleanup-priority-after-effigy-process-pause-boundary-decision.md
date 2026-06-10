# 2026-04-17 04:00:00 BST — Next Src Shell Cleanup Priority After Effigy Process Pause Boundary Decision

## Summary

Completed `234` by choosing the next substantial `/src` cleanup priority from
the `g02.017` queue after pausing process supervision.

## Decision

Choose **UI / widget primitive extraction** (g02.017 queue job #6) next.

Target: a new `effigy-ui` crate, not `effigy-core`.

Reason:

- `src/ui/**` is ~575 lines across renderer trait, theme/color handling,
  PlainRenderer implementation, progress/spinner, and table rendering — a
  real subsystem imported by 47 caller files
- widget data types (`NoticeLevel`, `TableSpec`, `KeyValue`, `MessageBlock`,
  `StepState`, `SummaryCounts`) already live in `effigy-core`; moving
  presentation logic there would mix pure data with rendering
- `effigy-core` currently has zero deps. Adding `anstream`, `anstyle`,
  `indicatif`, `tabled` would pull heavy presentation concerns into the pure
  core that `effigy-manifest`, `effigy-bootstrap`, `effigy-process`, and
  `effigy-cli` already depend on
- the honest call is therefore a new `effigy-ui` crate that depends on
  `effigy-core` for widget data types
- demo and docs remain under parallel-thread churn; UI extraction is disjoint
  from both and from the paused process subsystem

## Next Task

Execute
[`235-implement-effigy-ui-subsystem-extraction.md`](../../../specs/batch-cards/235-implement-effigy-ui-subsystem-extraction.md)
to move `src/ui/**` into the new `effigy-ui` crate.
