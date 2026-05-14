# g05.015 - Active Docs Reference Refresh And g05 Closeout

Status: Complete
Depends on: `g05.009`, `g05.010`, `g05.011`, `g05.012`, `g05.013`, `g05.014`

## Goal

Refresh active roadmap and audit references after the cleanup suite lands, then
close the reopened `g05` queue with no dead planning pointers left behind.

## Evidence

- active docs still point at `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`
  even though that file now lives under `docs/specs/archive/`
- the reopened `g05` queue needs an explicit closeout file instead of another
  implicit generation stop

## Scope

- update active audit and roadmap docs that point at missing spec paths
- replace them with current contract references or explicit archived pointers
- confirm roadmap front doors match the final `g05` state
- close the reopened generation explicitly once the queued cleanup work lands

## Non-Goals

- no architecture rewrite
- no new cleanup work disguised as closeout
- no generation rollover by default inside this roadmap

## Acceptance Criteria

- active docs no longer point at missing spec paths
- roadmap front doors and generation index agree on final `g05` posture
- the reopened cleanup suite has an explicit closeout record

## Suggested Validation

- `effigy docs check paths`
- `effigy docs check links`
- docs review

## Next Task

No next task. `g05` cleanup closeout is complete.
