# 11 Demo Post-Artifact Follow-Up Boundary Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.17`

## Context

Effigy already ships the first demo browser foundation plus artifact-opening
affordances. The remaining question was whether a tighter browser follow-up
still existed before live log visibility, or whether logs were now the next
honest operator need exposed by the shipped demos.

## Decision

Choose live log visibility as the next bounded browser slice.

## Why

- `browser-proof-report` already proves the browser can navigate and open
  meaningful proof artifacts
- `lifecycle-window` exposes the next real operator gap as current activity
  visibility while a demo is running
- the gap is now "show me recent proof output here" rather than another
  inventory or artifact affordance

## Boundary

In scope next:

- bounded recent-output visibility inside `effigy demo browser`
- active-attempt output when runner-owned logs exist
- latest terminal output when available for completed attempts

Still deferred:

- terminal emulation
- generic runtime cancellation expansion
- multi-attempt history
- desktop-client decisions

## Validation Performed

- `git diff --check`
- `effigy qa:docs`

## Next Task

Implement bounded live log visibility through
[`../../specs/batch-cards/024-implement-demo-browser-live-log-visibility.md`](../../specs/batch-cards/024-implement-demo-browser-live-log-visibility.md).
