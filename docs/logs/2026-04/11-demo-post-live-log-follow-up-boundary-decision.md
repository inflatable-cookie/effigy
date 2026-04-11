# 11 Demo Post-Live-Log Follow-Up Boundary Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.19`

## Context

Effigy now ships the first honest demo browser foundation plus artifact access
and bounded recent-output visibility. The remaining question was whether the
next browser slice should deepen log handling, polish artifact/detail
inspection, or address a tighter browse/discovery gap first.

## Decision

Choose bounded browser query controls as the next slice.

## Why

- the browser already supports grouped list/detail browsing, lifecycle actions,
  artifact opening, and recent output for the selected demo
- the current TUI still cannot narrow the registry by search, owner, status,
  gap, or stale state without leaving the browser
- the shipped `effigy demo list` contract already defines those query
  semantics, so the browser can reuse an existing runner-owned model instead of
  inventing a new one
- richer log handling would widen quickly into tailing, stream behavior, or
  history questions that the current evidence does not justify as the next
  slice
- artifact/detail polish remains useful, but the more immediate operator gap is
  browseability once the registry grows beyond the two self-hosted demos

## Boundary

In scope next:

- bounded in-browser query controls
- visible query state
- empty-result handling
- reuse of the existing `demo list` query dimensions

Still deferred:

- richer live-log handling beyond bounded recent output
- artifact preview or richer rendering
- terminal emulation
- broader generic runtime cancellation
- multi-attempt history
- desktop-client decisions

## Validation Performed

- `git diff --check`
- `effigy qa:docs`

## Next Task

Implement bounded browser query controls through
[`../../specs/batch-cards/026-implement-demo-browser-query-controls.md`](../../specs/batch-cards/026-implement-demo-browser-query-controls.md).
