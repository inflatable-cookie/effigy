# Demo Active-Attempt, Stop, And Rerun Contract Decision

Date: 2026-04-11
Roadmap: `g02.003`

## Summary

Locked the first lifecycle-control contract for Effigy's demo runner.

This batch separated immutable terminal proof receipts from mutable in-flight
runner state, fixed the first CLI targeting rule around demo ids, and bounded
the next execution slice so Effigy only promises stop support where it
actually owns a cancellable runtime handle.

## Decided

- active attempts are a separate runner-owned state layer, not a mutation of
  latest-attempt receipts
- the first lifecycle contract allows at most one active attempt per demo
- `demo stop` and `demo rerun` target demo ids in the first CLI grammar
- attempt ids are still required in runner state and inspection data for
  provenance, but not yet in the top-level command shape
- `demo rerun <id>` is a fresh-attempt command and must fail fast if the demo
  is still actively running
- the next implementation slice must not pretend every task-backed demo is
  stoppable if Effigy does not yet own a cancellable handle for that runtime

## Why This Boundary Matters

- `demo run` already creates terminal receipts, but that is not enough to model
  `running now`
- browser/TUI state later needs truthful active lifecycle data
- stop/rerun semantics become fragile fast if receipts and process state are
  treated as the same record

## Validation

- `git diff --check`
- `effigy qa:docs`

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Moved: `demo run without lifecycle-control contract -> explicit active-attempt, stop, and rerun targeting model`
- Remaining open: implementation of active-attempt state, honest stop support for runner-owned attempts, broader stoppability beyond directly controlled runtimes, and the later TUI/browser client

## Next Task

Use the next `g02.003` ready card to implement the first bounded
active-attempt, stop, and rerun slice.
