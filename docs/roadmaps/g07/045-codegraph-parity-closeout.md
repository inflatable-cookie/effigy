# g07.045 - CodeGraph Parity Closeout

Status: Complete
Depends on: `g07.036` through `g07.044`

## Goal

Close the parity suite with evidence about where Effigy equals, beats, or still
lags CodeGraph for agent navigation.

## Scope

- rerun the full `g07.036` benchmark harness
- report:
  - tool-call deltas
  - file-read deltas
  - elapsed time
  - output byte costs
  - cold/warm index split
  - false positives and false negatives
- compare Effigy against CodeGraph claims by category, not marketing headline
- decide whether remaining gaps are:
  - roadmap-ready inside `g07`
  - future generation work
  - explicit non-goals
- close or re-scope strict lane `091`
- refresh front doors:
  - `docs/roadmaps/g07/README.md`
  - `docs/roadmaps/generation-index.md`
  - `docs/specs/README.md`
  - relevant docs/skill/changelog surfaces

## Guardrails

- no unsupported parity claim
- no hiding known weak queries
- no leaving a ready card active after closeout
- no release-readiness claim unless release gates are separately run

## Acceptance Criteria

- closeout log exists with benchmark tables and conclusions
- every roadmap in the suite is complete, superseded, or explicitly deferred
- currentness surfaces point to the next real task
- `effigy graph` guidance reflects measured behavior

## Evidence

- [`2026-05/18-174500-codegraph-parity-closeout.md`](../../logs/archive/2026-05/18-174500-codegraph-parity-closeout.md)

## Next Task

No active ready card. Open a bounded follow-up planning lane for graph query
latency and fixture-backed parity execution before more parity work.
