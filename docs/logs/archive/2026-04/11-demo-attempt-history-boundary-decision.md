# Demo Attempt History Boundary Decision

Date: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Summary

The next bounded `g02.003` slice is runner-side attempt history, not more
browser work.

Effigy's first browser is now good enough to stop widening for the moment. The
main remaining product gap is that the runner still treats demo history as
"active attempt plus latest terminal attempt" only. That is enough for basic
status and stop/rerun semantics, but not enough for meaningful proof review or
for understanding what happened before the most recent run.

## Decision

- persist a bounded per-demo history of terminal attempts
- keep the first slice inspect-first: richer `demo inspect` text/JSON output
  before any list or browser history rendering
- retain latest-attempt compatibility as the short summary surface
- defer list/browser history rendering until the runner-owned history surface is
  real

## Bounded Contract

The first history slice should record only compact terminal outcomes:

- terminal status
- timestamp or ordering information
- summary text
- receipt/artifact references when present

It should not widen into:

- multi-attempt concurrent execution
- queueing or generic runtime orchestration
- richer log streaming
- more browser layout churn

## Vision Target Delta

- Primary Tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: `next slice undecided after browser baseline` -> `bounded persisted
  attempt history chosen; inspect-first foundation activated`
- Remaining Open:
  - exact storage shape and cap for bounded history
  - whether the first history slice should later expand into `demo list`, the
    browser, or a dedicated query surface

## Validation

- `git diff --check`
- `effigy qa:docs`

## Next Task

Execute [`033-implement-demo-attempt-history-foundation.md`](../../../specs/batch-cards/033-implement-demo-attempt-history-foundation.md)
to deliver bounded persisted terminal-attempt history and enriched `demo
inspect` output before widening any browser or list surfaces.
