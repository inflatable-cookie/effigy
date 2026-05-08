# 278 Implement Generated-Compose Shared Services

Status: archived
Updated: 2026-04-18
Roadmap: `g02.016`
Spec: `docs/specs/016-multi-project-coordination-strict-lane.md`

## Objective

Close `g02.016` by making one bounded shared-service product path real for
Effigy-owned generated compose.

## Context

`272` landed cross-project status, `274` landed generated-compose port
auto-allocation, and `276` landed bounded resource stats. The remaining
resource-efficiency move is optional shared backing services, but only on a
boundary Effigy can actually own safely.

The substrate already exists:

- generated compose ownership in `effigy-containers`
- stable shared port allocation through `PortRegistry`
- the normal `container up/down/status` lifecycle surface
- standard app env conventions already documented in the catalog override
  example (`DB_HOST`, `DB_PORT`, `REDIS_HOST`, `MEMCACHED_HOST`, and similar)

## In Scope

- add manifest-owned `shared = true` support for generated-compose backing
  services
- support that path only for standalone catalogs that can run as shared
  instances without generated Dockerfiles or rendered config files:
  `mariadb`, `postgres`, `redis`, and `memcached`
- start and reuse shared instances from the normal `container up` path using
  stable host-port assignments
- rewrite generated consumer compose so shared services are removed from the
  local stack and remaining services get standard host/port environment
  variables pointing at the shared instance
- reflect the bounded shared-service state in container policy/status/help/docs
- add focused coverage in the affected manifest/container/runner/doctor
  surfaces

## Out Of Scope

- shared services for direct `compose_file` ownership
- shared services for catalogs that need generated Dockerfiles or rendered
  config files
- explicit shared-service lifecycle commands, garbage collection, or refcounts
- gateway integration for shared services
- trying to auto-own every framework-specific credential or DSN convention

## Acceptance

- generated-compose containers can declare supported backing services with
  `shared = true`
- `container up` reuses one shared instance across repos when the shared
  service declaration matches
- the consumer stack no longer starts a duplicate local copy of that shared
  service
- remaining app services receive honest standard host/port env variables that
  point at the shared instance
- focused tests cover manifest acceptance, generated compose rewrite, shared
  instance reuse, and status/report shaping

## Result

This batch is now landed.

What changed:

- generated-compose manifests can now mark supported backing services with
  `shared = true`
- Effigy now resolves those shared declarations into stable shared compose
  projects under `~/.effigy/shared-services/` with stable host-port
  allocation from the existing registry
- `container up` now ensures those shared instances are running before the
  consumer stack starts, while generated consumer compose drops the local
  duplicate service and injects standard host/port env vars that point at the
  shared target
- `container down/reset/status` and related text/JSON shaping now report the
  bounded shared-service state honestly, including the explicit leave-running
  behavior for this batch
- direct `compose_file` ownership, unsupported catalogs, generated Dockerfile
  cases, and explicit shared-service teardown all remain out of scope on
  purpose

## Next Task

No further execution lives on this card. Close `g02.016` on this shipped
boundary, then reopen planning around `g02.015`.
