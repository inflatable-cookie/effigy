# Demo Post-Lifecycle Follow-Up Boundary Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.10`

## Summary

Decided that the next bounded demo-runner slice should prioritize
browser-facing state/query polish, not broader stoppability/runtime expansion.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `two plausible post-lifecycle directions` to
  `one explicit next slice: browser-state/query polish`
- Remaining open:
  - broader stoppability once runtime-owned cancellable handles exist
  - TUI/browser implementation
  - consumer-repo rollout work

## Decision

Chose browser-state/query polish next because it builds directly on the shipped
demo registry, run, inspect, stop, and rerun surfaces without over-claiming
runtime control that Effigy still does not own.

Deferred broader stoppability because it immediately runs into a deeper runtime
question:

- run-backed demos are now honestly stoppable because Effigy owns the process
  handle
- generic task-backed stoppability still depends on cancellable task/runtime
  handles that do not exist yet

That makes browser-state polish the clean next slice and broader stoppability a
separate runtime-boundary lane, not something to blur together in one batch.

## Boundaries For The Next Slice

The next execution batch should focus on:

- browser-row-friendly list output
- focused discovery/inspection query polish
- explicit active/base/freshness/gap state presentation
- receipt/artifact drilldown clarity

The next execution batch should not:

- implement TUI rendering
- broaden task-backed cancellation promises
- add multi-attempt history or queueing

## Next Task

Execute the next `g02.003` ready card for browser-facing state/query polish,
then reopen broader stoppability only as a separate runtime-handle planning
question.
