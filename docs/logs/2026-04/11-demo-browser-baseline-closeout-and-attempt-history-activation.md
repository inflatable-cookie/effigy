# Demo Browser Baseline Closeout And Attempt History Activation

Date: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Summary

The first demo browser is now treated as sufficient baseline product surface.
It already covers list/detail browsing, lifecycle actions, filtering/grouping,
artifact opening, self-hosted proof demos, and a calmer panel-focused operator
flow.

That means the next honest gap is no longer another browser-only ergonomic
slice. The runner still only exposes one active attempt and one latest
terminal attempt, which is too thin for meaningful result history, richer
inspection, or future browser/CLI views beyond "what happened most recently."

## Decision

- mark the stale post-metadata-query browser decision as complete
- stop widening the first browser through more local UI cleanup
- activate a new ready card around persisted attempt history and result
  timelines as the next runner-side `g02.003` question

## Why This Boundary

- the browser now has enough operator-visible affordances to serve as the first
  interactive client
- recent cleanup proved that further work on the detail pane quickly becomes
  presentation churn unless the runner owns richer result state underneath it
- attempt history is useful across `demo inspect`, future browser views, and
  proof review without forcing more UI-first decisions

## Vision Target Delta

- Primary Tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: `browser follow-up decision still pending` -> `browser baseline
  accepted; runner-side attempt-history decision activated`
- Remaining Open:
  - exact bounded shape for persisted attempt history
  - whether the first history slice is inspect-only or also affects list output

## Validation

- `git diff --check`
- `effigy qa:docs`

## Next Task

Execute [`032-decide-demo-attempt-history-and-result-timeline-boundary.md`](../../specs/batch-cards/032-decide-demo-attempt-history-and-result-timeline-boundary.md)
to lock the first bounded runner-side history slice before more demo UI or
repo adoption work starts.
