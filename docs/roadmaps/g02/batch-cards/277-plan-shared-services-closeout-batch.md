# 277 Plan Shared Services Closeout Batch

Status: archived
Updated: 2026-04-18
Roadmap: `g02.016`
Spec: `docs/specs/016-multi-project-coordination-strict-lane.md`

## Objective

Choose the final bounded `g02.016` execution batch now that status,
generated-compose auto-allocation, and bounded resource stats are all landed.

## Scope

- assess whether shared services still earn a real product batch after `276`
- decide the smallest trustworthy shared-service boundary Effigy can own
- record what must stay out of the batch so the lane closes cleanly
- refresh the front-door planning surfaces so `continue` resolves to one
  explicit final execution card again

## Out Of Scope

- implementing the next batch itself
- `g02.015` persistent data lifecycle work
- broad reconsideration of the full `g02` spine

## Acceptance

- one explicit next execution card exists for `g02.016`
- the chosen shared-service batch is bounded on a real product surface rather
  than the older roadmap aspiration
- the front-door planning surfaces stop leaving the final `g02.016` move
  ambiguous

## Decision

`g02.016` should finish with one bounded shared-services batch, not a deferral.

Why this still earns a batch:

- the lane now has cross-project visibility, collision avoidance, and bounded
  resource stats, so the remaining coordination value is the actual
  resource-efficiency move the roadmap originally promised
- shared services are the only remaining part of `g02.016` that changes
  operator behavior rather than just observing it
- the product already owns generated compose, port allocation, and container
  lifecycle enough to ship one narrow shared-service integration slice

The trustworthy product boundary is narrower than the original roadmap text:

- support shared services only for Effigy-owned generated compose, not direct
  `compose_file` ownership
- support only backing-service catalogs that can run as standalone shared
  instances without generated Dockerfiles or config artifacts:
  `mariadb`, `postgres`, `redis`, and `memcached`
- start and reuse shared instances on demand from the normal `container up`
  path using stable host-port assignments from the shipped shared registry
- rewrite generated consumer compose to drop those local service definitions
  and inject standard host/port environment variables that point remaining
  app services at the shared instance
- keep shutdown simple and honest: `container down/reset` should not tear
  shared instances back down in this batch

What stays out:

- shared services for direct `compose_file` containers
- shared services for catalogs that need generated Dockerfiles, rendered config
  files, or deeper template surgery
- reference counting, garbage collection, or explicit shared-service lifecycle
  commands
- trying to make every app framework auto-discover credentials or DSNs beyond
  the standard host/port env injection Effigy can own cleanly
- gateway integration for shared services

## Result

The final explicit `g02.016` execution batch is now card `278`.

## Next Task

Execute `278` to land bounded generated-compose shared services and close
`g02.016` on that boundary.
