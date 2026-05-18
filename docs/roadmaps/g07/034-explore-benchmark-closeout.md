# g07.034 - Explore Benchmark Closeout

Status: Complete
Depends on: `g07.031` through `g07.033`

## Goal

Close the lane with evidence about whether `graph explore` materially improves
day-to-day agent navigation.

## Scope

- rerun the benchmark tasks from `g07.031`
- compare:
  - current `graph context -> open files -> rg`
  - new `graph explore -> targeted verification`
  - direct `rg` for exact-match tasks
- record tool-call count, file-read count, and elapsed time
- identify where `explore` wins, ties, or loses
- tune docs to match the evidence

## Guardrails

- no marketing-style percentage claim unless the measurement really supports it
- benchmark exact-match work separately from architecture/navigation work
- report cold-index and warm-index posture separately
- keep failures visible as follow-up candidates rather than smoothing them over

## Acceptance Criteria

- benchmark log records before/after results
- tests pass for the implemented command and contract
- docs do not overstate the feature
- lane `090` has no active ready card left

## Evidence

- [`2026-05/18-133020-graph-explore-implementation-closeout.md`](../../logs/2026-05/18-133020-graph-explore-implementation-closeout.md)

## Next Task

No follow-up task selected until benchmark evidence identifies one.
