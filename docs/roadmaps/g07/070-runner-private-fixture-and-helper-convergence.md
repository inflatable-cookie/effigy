# g07.070 - Runner Private Fixture And Helper Convergence

Status: Planned
Depends on: `g07.069`

## Goal

Trim the remaining high runner-private duplication clusters where the helper
shape is already obvious and local reuse is real.

## Evidence

The duplicate scan still shows high or repeated runner-private duplication
around:

- temp-repo setup in container-command test surfaces
- local vault/test-secret setup patterns
- a few repeated builder fragments that already share one domain

## Scope

- converge only private helper paths with obvious local ownership
- keep helpers close to the affected runner module or test family
- avoid cross-crate support layers unless two crates truly need the same shape

## Guardrails

- no public helper API for internal test convenience
- no fixture abstraction that hides scenario setup
- no runner-wide helper cleanup tour

## Acceptance Criteria

- the current high temp-repo duplicate is removed or clearly justified
- touched tests stay readable
- helper placement matches ownership instead of convenience

## Next Task

After this lands, proceed to [`071-residual-maintainability-closeout.md`](./071-residual-maintainability-closeout.md).
