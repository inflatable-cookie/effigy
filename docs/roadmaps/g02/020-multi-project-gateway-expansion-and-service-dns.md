# 020 - Multi-Project Gateway Expansion And Service DNS

Generation: `g02`

Status: Complete
Owner: Platform
Created: 2026-04-20
Depends on: 013, 014, 016

## Vision Alignment

Effigy can now bring up container systems, register HTTP routes through the
gateway, and coordinate multiple repos well enough to avoid some host-port
collisions.

That is not yet the same thing as a scalable multi-project local network.

Once several projects run simultaneously, the remaining cracks are obvious:

- project-owned HTTP services still leak port-management detail into the
  runtime path
- TCP services like postgres and redis still do not have first-class `.test`
  identities
- shared services exist, but the gateway/DNS model still thinks mostly in
  terms of per-project HTTP proxy routes

This roadmap closes that gap without replacing the current gateway/container
architecture.

## Primary Tags

- `GATEWAY`
- `CONTAINERS`
- `DX`

## Target Envelope

- HTTP services stop depending on manually declared host ports for gateway
  registration
- TCP services gain canonical `.test` hostnames on a stable loopback-IP model
- multi-project local networking scales without per-project port juggling
- shared services remain bounded but fit the new DNS model honestly
- the gateway lifecycle stays on-demand rather than becoming a permanent
  background daemon

## Vision Target Delta

- Move from `bounded HTTP gateway plus partial port coordination` toward
  `one scalable local network model for HTTP and TCP services across many
  simultaneous projects`.

## Problem

`g02.014` and `g02.016` shipped the right substrate, but they intentionally
stopped short of the full operator outcome.

The remaining issues are:

- HTTP routes still assume known published host ports at registration time
- TCP services still force developers to track ephemeral ports or manual
  mappings instead of using stable names
- project isolation is incomplete because standard service ports collide
  without extra user shaping
- the gateway route model cannot yet express "DNS answer only" service records
  that should not be reverse proxied

That leaves Effigy's local-network story good enough for one project, but not
clean enough for many.

## Goals

- add per-route DNS target support through `dns_ip`
- assign stable loopback IPs for project-owned service groups
- bind TCP service ports on those per-project loopback IPs while keeping
  standard container ports standard
- switch HTTP gateway registration to post-start published-port discovery
  instead of manifest-declared host ports
- derive canonical TCP service hostnames from service catalogs
- keep shared services on the same operator-facing naming model, even when
  multiple projects point at one shared backing instance

## Non-Goals

- this roadmap does not replace the current reverse-proxy gateway model
- this roadmap does not add Linux or Windows resolver integration
- this roadmap does not make TCP service DNS names user-configurable in v1
- this roadmap does not add refcounted shared-service teardown or garbage
  collection
- this roadmap does not widen into a permanent gateway daemon or launchd-owned
  lifecycle

## Contract Direction

### 1. Route Model

`effigy-gateway` route entries gain `dns_ip: Option<Ipv4Addr>`.

- HTTP routes keep `proxy_target` and usually leave `dns_ip` unset
- TCP service routes set `dns_ip` and omit `proxy_target`
- DNS answers use `route.dns_ip` when present, else fall back to
  `config.resolve_to`
- the reverse proxy ignores routes without `proxy_target`

This is the foundation batch. Everything else depends on it.

### 2. Loopback IP Allocation

Project-owned service groups get stable loopback IP assignments in a bounded
`127.1.0.x` pool persisted in Effigy state.

- assignments survive restarts
- gateway setup pre-allocates the alias range during the existing elevated
  setup step
- TCP service port publication binds to the assigned loopback IP rather than
  `127.0.0.1`

### 3. HTTP Services

HTTP services stop requiring explicit host-port declarations for gateway use.

- generated compose uses ephemeral host publication for HTTP ports
- after `docker compose up`, Effigy discovers the effective host port through
  runtime inspection
- gateway registration uses the discovered host port
- developers stay on `.test` domains and never need to care about the
  published port

### 4. TCP Service DNS

Effigy derives canonical service names from the catalog:

- `postgres` and `mysql`/`mariadb` -> `db.<app>.test`
- `redis` -> `redis.<app>.test`
- `elasticsearch` -> `search.<app>.test`
- `minio`/`s3` -> `s3.<app>.test`
- mail UI stays on HTTP naming such as `mail.<app>.test`
- mail SMTP uses `smtp.<app>.test`

These records resolve directly to the relevant loopback IP. They are DNS
routes, not proxy routes.

### 5. Shared Services

Shared services follow the same operator-facing DNS model, but reuse one
backing service identity underneath when multiple projects consume the same
shared instance.

Settled v1 decision:

- project-facing aliases like `db.app1.test` and `db.app2.test` may both
  resolve to one shared-service loopback IP
- the default "one project, one IP" model applies to project-owned services,
  not as a hard invariant for every route in the system
- Effigy should prefer compatibility in naming over duplicating one shared
  backing service onto many project IPs

## Workstreams

### 1. Route And DNS Foundation

Primary write set:

- `crates/effigy-gateway/src/routes.rs`
- `crates/effigy-gateway/src/dns.rs`
- route serialization tests and integration coverage

Scope:

- add `dns_ip`
- update resolver behavior
- keep proxy behavior unchanged for HTTP routes

### 2. Loopback Assignment And Gateway Setup

Primary write set:

- `crates/effigy-gateway/**`
- `src/runner/gateway_command.rs`
- gateway setup/state tests

Scope:

- persist loopback-IP assignments
- provision the alias range during gateway setup
- keep the on-demand gateway lifecycle intact

### 3. Container Registration Rewrite

Primary write set:

- `src/runner/container_command/gateway_registration.rs`
- `crates/effigy-containers/**`
- container lifecycle tests

Scope:

- stop assuming manifest-declared host ports for HTTP registration
- discover effective published HTTP ports after startup
- derive TCP service DNS registrations from container/service declarations

### 4. Shared-Service Integration

Primary write set:

- shared-service shaping in `effigy-containers`
- gateway registration and route ownership
- manifest/env injection surfaces where needed

Scope:

- make shared services register honest DNS aliases
- keep the current bounded shared-service runtime model
- inject readable service host env vars where the service catalog already
  implies them

## Exit Condition

This roadmap is complete when:

- HTTP services register clean gateway routes without manual host-port
  declarations
- TCP services expose stable `.test` names on the loopback-IP model
- multiple projects can run simultaneously without service-port conflict churn
- shared services fit the same naming story on an honest bounded runtime path

## Next Task

This roadmap is complete. `303`, `306`, `307`, `308`, `309`, and `310` have
all landed. The lane now has both sides of the real-project proof:
`underlay-reference` for project-owned aliases and `contactpatch` for shared
service aliases.

`g02.007` and `g02.019` are queued behind this lane as of 2026-04-22. See
`docs/logs/archive/2026-04/22-190000-g02-020-re-sequencing-ahead-of-g02-007-and-g02-019.md`
for the re-sequencing rationale.
