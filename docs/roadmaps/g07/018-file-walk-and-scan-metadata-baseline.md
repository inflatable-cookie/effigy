# g07.018 - File Walk And Scan Metadata Baseline

Status: Complete
Depends on: `g07.017`

## Goal

Separate extractor cost from scan cost so the next optimization work targets
the real floor instead of guessing.

## Scope

- measure full-repo file walk time directly
- measure scan-entry creation and metadata collection cost
- record how often `graph index`, `status`, and stale detection each walk the
  repo in the no-op case
- identify the first obviously duplicated scan passes worth removing

## Guardrails

- no code motion just to claim a measurement milestone
- no benchmark theater with overlapping commands or polluted timings
- do not infer scan dominance without direct evidence

## Acceptance

- one baseline log records the walk/scan shape clearly enough to drive `952`

## Next Task

No active task remains in this roadmap.
