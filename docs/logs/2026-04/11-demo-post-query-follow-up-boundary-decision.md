# 11 Demo Post-Query Follow-Up Boundary Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.21`

## Context

Effigy now ships the first honest demo browser with list/detail browsing,
lifecycle actions, artifact opening, bounded recent output, and in-browser
query controls. The remaining question was whether the next slice should deepen
browse ergonomics further, widen into richer log/detail rendering, or address a
tighter browser usability gap first.

## Decision

Choose bounded detail-pane navigation as the next slice.

## Why

- the current detail pane is still a static paragraph with no navigation
- the shipped self-hosted demos already generate enough content across
  artifacts, receipt summary, and recent output that lower sections can become
  unreachable in a normal terminal viewport
- query controls reduce the need to leave the browser for discovery, so the
  next real bottleneck is traversing the selected record itself
- bounded detail navigation stays inside TUI view ergonomics and does not
  widen into richer log streaming, artifact rendering, or terminal behavior

## Boundary

In scope next:

- bounded detail-pane scrolling/navigation
- visible detail-position feedback when useful
- preserving artifact selection semantics while the pane moves

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

Implement bounded detail-pane navigation through
[`../../specs/batch-cards/028-implement-demo-browser-detail-navigation.md`](../../specs/batch-cards/028-implement-demo-browser-detail-navigation.md).
