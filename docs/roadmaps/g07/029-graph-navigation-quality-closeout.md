# g07.029 - Graph Navigation Quality Closeout

Status: Complete
Depends on: `g07.026` through `g07.028`

## Goal

Close the ranking-quality lane with honest proof of where graph navigation saves
agent time and where it does not.

## Scope

- rerun all gold tasks after ranking/snippet changes
- compare:
  - `graph context`
  - `graph search`
  - direct `rg`
- record rank changes, timings, and residual misses
- update the agent skill and graph guide if the recommended workflow changes
- decide whether graph ranking is good enough to park or needs another tranche

## Acceptance Criteria

- closeout log exists with before/after top-file evidence
- no active ranking-quality card remains
- residual limitations are explicit
- CI-relevant tests are green

## Next Task

No active ranking-quality task remains.
