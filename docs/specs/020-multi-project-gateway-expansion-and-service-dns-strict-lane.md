# 020 Multi-Project Gateway Expansion And Service DNS Strict Lane

Status: active
Updated: 2026-04-22
Roadmap: `g02.020`

## Context

The gateway and multi-project lanes are complete on their bounded v1 surfaces,
but the local-network story still stops short of the scalable shape the
product now wants.

Effigy can already:

- run multiple generated-compose projects simultaneously
- auto-allocate host ports to avoid some collisions
- register HTTP routes through the shared gateway
- expose bounded shared backing services

What it still cannot do cleanly:

- give TCP services first-class `.test` identities
- stop depending on declared host ports for HTTP route registration
- express DNS-only service routes that should resolve but not reverse proxy

This lane owns that follow-through without replacing the current
gateway/container architecture.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/020-multi-project-gateway-expansion-and-service-dns.md`
- `docs/architecture/020-container-infrastructure-design.md`
- `docs/logs/2026-04/20-120000-multi-project-gateway-expansion-handoff.md`

## Lane Focus

This lane owns:

- per-route DNS target support in the gateway route model
- project and shared-service loopback-IP assignment on the bounded macOS path
- HTTP post-start published-port discovery for gateway registration
- canonical TCP service DNS aliases derived from the shipped service catalogs
- bounded shared-service integration with the same operator-facing naming
  model

## Current Posture

`active`

The product substrate this lane builds on is already real:

- `g02.014` shipped the host-native gateway command, route table, DNS
  responder, reverse proxy, and container lifecycle route registration
- `g02.016` shipped generated-compose host-port auto-allocation, cross-project
  status, route dashboarding, and bounded shared services
- `g02.007` is now queued behind this lane, still parked on its own explicit
  release-intent gate
- `g02.019` remains planned, queued behind this lane's exit

This lane was re-sequenced ahead of `g02.007` and `g02.019` on 2026-04-22
because the multi-project port-collision and TCP service DNS gaps are causing
concrete, daily consumer-repo friction (see
`docs/logs/2026-04/22-190000-g02-020-re-sequencing-ahead-of-g02-007-and-g02-019.md`).
The route-model and loopback-setup batches have now landed, so execution
resumes from the HTTP registration follow-up without reconstructing earlier
foundations.

Settled design decisions carried into execution:

- route entries now carry `dns_ip: Option<Ipv4Addr>`
- HTTP routes continue to proxy; TCP service routes resolve in DNS only
- project-owned services default to one stable loopback IP per project
- shared-service aliases may collapse onto one shared backing-service IP when
  several projects consume the same shared instance

## Integration Constraint

This lane should execute in bounded batches:

- land the route/DNS model first because every later batch depends on it
- keep HTTP post-start port discovery separate from loopback-IP allocation so
  regressions stay attributable
- make shared-service DNS reuse follow the base route model rather than
  inventing special-case registration first
- do not let the broader local-network goal reopen release-prep or the
  `g02.019` audit lane

## Continuation Chain

The execution order on this active lane:

1. `301` — complete. The route-model foundation landed: `dns_ip` is part of
   the route shape, DNS resolution uses it when present, and proxy behavior
   stays honest for HTTP-only routes
2. `302` — complete. The post-`301` decision point chose loopback-IP
   allocation before HTTP post-start port discovery
3. `303` — complete. Loopback-IP allocation now persists in gateway state, and
   the bounded macOS alias range is provisioned during the existing elevated
   gateway setup path
4. `306` — complete. HTTP gateway registration now resolves targets from live
   runtime published ports instead of assuming manifest-declared host ports on
   the generated-compose path
5. `307` — complete. Project-owned TCP services now register bounded DNS-only
   aliases on the loopback-IP foundation
6. `308` — complete. Shared-service DNS aliases now reuse one bounded
   loopback-IP identity per shared backing service while explicit manifest
   routes and project-owned aliases keep precedence
7. `309` — next execution. Prove the shipped HTTP and TCP service DNS model
   in one real consumer repo and migrate that repo off hardcoded local
   service ports where the alias model now covers the runtime path
8. later — widen consumer-repo migration beyond the first proof repo

## Exit Condition

This strict lane is complete when:

- the gateway route model can express DNS-only service routes cleanly
- HTTP services no longer require declared host ports for gateway registration
- TCP services expose stable `.test` names on the bounded loopback-IP model
- shared services fit the same naming model honestly on the shipped bounded
  runtime path

## Next Task

Execute `309` — prove the shipped service DNS model in one real consumer repo
and migrate that repo off hardcoded local service ports where the bounded
alias path now covers the runtime.

See
`docs/specs/batch-cards/309-prove-service-dns-aliases-in-one-real-project.md`.
