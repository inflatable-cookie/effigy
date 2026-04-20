# 020 Multi-Project Gateway Expansion And Service DNS Strict Lane

Status: staged
Updated: 2026-04-20
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

`staged`

The product substrate this lane builds on is already real:

- `g02.014` shipped the host-native gateway command, route table, DNS
  responder, reverse proxy, and container lifecycle route registration
- `g02.016` shipped generated-compose host-port auto-allocation, cross-project
  status, route dashboarding, and bounded shared services
- the active release lane remains `g02.007`
- the next post-release audit/documentation lane remains `g02.019`

This lane is intentionally staged behind those higher-priority fronts, but the
first route-model batch has now landed in a parallel thread so a later resume
does not need to reconstruct the opening move.

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

## Staged Continuation Chain

The intended execution order starts with:

1. `301` — complete. The route-model foundation landed: `dns_ip` is part of
   the route shape, DNS resolution uses it when present, and proxy behavior
   stays honest for HTTP-only routes
2. `302` — complete. The post-`301` decision point now chooses loopback-IP
   allocation before HTTP post-start port discovery
3. `303` — next execution. Land loopback-IP allocation and gateway setup
   integration on the bounded macOS path
4. later execution — HTTP post-start published-port discovery for gateway
   registration
5. later execution — container registration rewrite for TCP service alias
   registration and shared-service DNS reuse

## Exit Condition

This strict lane is complete when:

- the gateway route model can express DNS-only service routes cleanly
- HTTP services no longer require declared host ports for gateway registration
- TCP services expose stable `.test` names on the bounded loopback-IP model
- shared services fit the same naming model honestly on the shipped bounded
  runtime path

## Next Task

Keep `g02.007` as the live lane and `g02.019` as the next post-release audit
lane.

When `g02.020` is resumed after those fronts settle, execute `303` to land the
loopback-IP allocation and gateway setup foundation.
