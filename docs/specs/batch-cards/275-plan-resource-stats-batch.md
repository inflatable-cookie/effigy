# 275 Plan Resource Stats Batch

Status: landed
Updated: 2026-04-18
Roadmap: `g02.016`
Spec: `docs/specs/016-multi-project-coordination-strict-lane.md`

## Objective

Choose the next bounded `g02.016` follow-up now that cross-project status and
generated-compose port auto-allocation are both landed.

## Scope

- assess the remaining `g02.016` options after `274`
- decide whether the next bounded follow-up should be resource stats, shared
  services, or explicit deferral
- define the smallest trustworthy execution boundary for the chosen follow-up
- refresh the front-door planning surfaces so `continue` resolves to one
  explicit execution card again

## Out Of Scope

- implementing the next batch itself
- `g02.015` persistent data lifecycle work
- broad reconsideration of the full `g02` spine

## Acceptance

- one explicit next execution card exists for `g02.016`
- the chosen batch is bounded on a real product surface rather than roadmap
  ambition
- the front-door planning surfaces stop leaving the next `g02.016` move
  ambiguous

## Decision

The next bounded `g02.016` follow-up should be resource stats, not shared
services.

Why this is next:

- the coordination lane now has visibility plus collision avoidance, but still
  lacks one honest resource view across running Effigy-managed environments
- shared services widen mutation, lifecycle ownership, and isolation tradeoffs
  more than the lane currently needs
- container stats can stay read-only and product-owned on top of the same
  running-environment discovery that `272` already landed

The trustworthy product boundary is narrower than the roadmap wording:

- start with a read-only `effigy container stats --all` surface for running
  Effigy-managed containers across repos
- report CPU and memory using the container runtime's live stats output where
  available, grouped by repo and compose project
- keep gateway metrics, TUI views, history/recording, and shared-service
  lifecycle out of scope
- stay honest when the runtime cannot provide stats or when a discovered
  environment has already exited

What stays out of the next batch:

- shared service orchestration or `shared = true`
- historical metrics, polling daemons, or dashboards
- browser or terminal UI work
- broad mutation or scheduling based on resource usage

## Result

The next explicit `g02.016` execution batch is now card `276`.

## Next Task

Execute `276` to land bounded cross-project container resource stats.
