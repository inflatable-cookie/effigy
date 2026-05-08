# 273 Plan Port Auto-Allocation Batch

Status: archived
Updated: 2026-04-18
Roadmap: `g02.016`
Spec: `docs/specs/016-multi-project-coordination-strict-lane.md`

## Objective

Choose the next bounded `g02.016` execution batch now that the first
coordination visibility surface is landed.

## Scope

- assess the remaining `g02.016` options after `272`
- decide whether the next bounded follow-up should be port auto-allocation,
  resource stats, or explicit deferral
- define the smallest trustworthy execution boundary for the chosen follow-up
- refresh the front-door planning surfaces so `continue` resolves to one
  explicit next card again

## Out Of Scope

- implementing the next batch itself
- `g02.015` persistent data lifecycle work
- shared-service orchestration
- broad reconsideration of the full `g02` spine

## Acceptance

- one explicit next execution card exists for `g02.016`
- the chosen batch is bounded on a real product surface rather than roadmap
  ambition
- the front-door planning surfaces stop leaving the next `g02.016` move
  ambiguous

## Decision

The next bounded `g02.016` follow-up should be port auto-allocation, not
resource stats.

Why this is next:

- it addresses the original multi-project conflict problem directly, while
  stats are still a secondary observability layer
- the product already has `PortRegistry`, but it is only being consumed as a
  read-only coordination signal rather than a live allocation path
- the newly landed status/dashboard batch made the gap concrete: operators can
  now see cross-project environments, but Effigy still does not prevent
  duplicate host-port declarations from colliding

The trustworthy product boundary is narrower than the roadmap wording:

- auto-allocation should apply to Effigy-owned generated compose, where the
  product actually controls published port bindings
- direct `compose_file` ownership should stay explicit-ports-only for now,
  because Effigy does not own arbitrary user compose content and should not
  pretend it can rewrite it safely
- explicit manifest `host.ports` must still win unchanged
- allocations should persist in `~/.effigy/ports.json` so one project keeps a
  stable range across stop/start cycles instead of churning ports on each run

What stays out of the next batch:

- CPU or memory stats
- shared services
- allocation garbage collection or compaction
- trying to retrofit auto-allocation into direct user-owned compose files

## Result

The next explicit `g02.016` execution batch is now card `274`.

## Next Task

Execute `274` to land bounded port auto-allocation for Effigy-owned generated
compose environments.
