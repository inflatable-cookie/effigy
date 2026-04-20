---
title: multi-project gateway expansion — design handoff
status: active
owner: platform
updated: 2026-04-20
tags: [coordination, handoff, gateway, containers]
---

## What This Thread Was Doing

Design conversation about the next phase of the system/container feature.
`g02.014` (gateway) and `g02.016` (multi-project coordination) both shipped
with known gaps: port conflicts between projects, no clean host access to TCP
services, and auto-allocation that only partially solves the problem. This
thread worked through a concrete architecture for fixing those gaps without
dumping port management burden on the developer.

The main output is a set of design decisions — not code — that the
implementation thread should anchor on.

## Why It Matters

With 5+ projects running concurrently, the current model breaks. Every project
declares `8080:3000` and `5432:5432`; the second project to start wins or
fails unpredictably. The DNS/gateway story also stops at HTTP — postgres and
redis have no `.test` equivalent, so developers still need to track ephemeral
ports or hardcode connection strings. Solving this closes the last major
multi-project friction before the system feature is genuinely usable at
scale.

## Current State

- Done so far: design decisions reached and validated in conversation (see
  Important Context below). No code written.
- Still open: no roadmap entry exists for this work yet; nothing is
  implemented.
- Active spec lane: none — this needs a new roadmap card before execution
  starts.
- Canonical refs:
  - `docs/roadmaps/g02/014-rust-native-gateway.md` (gateway foundation,
    complete)
  - `docs/roadmaps/g02/016-multi-project-coordination.md` (multi-project
    coordination, complete on bounded surface)
  - `crates/effigy-gateway/src/dns.rs` — DNS resolver (resolve_to is
    currently a global static IP)
  - `crates/effigy-gateway/src/routes.rs` — route table model
  - `src/runner/container_command/gateway_registration.rs` — registration
    logic
  - `crates/effigy-containers/src/lib.rs` — EffectiveContainerPolicy
  - `crates/effigy-manifest/src/config_sections.rs` — manifest schema
- Remaining continuation envelope: open — implementation not started.
- Lane budget / pause signal: paused at design; waiting for roadmap card and
  implementation thread.
- Key files:
  - `/Users/tom/Dev/projects/effigy/crates/effigy-gateway/src/dns.rs`
  - `/Users/tom/Dev/projects/effigy/crates/effigy-gateway/src/routes.rs`
  - `/Users/tom/Dev/projects/effigy/src/runner/container_command/gateway_registration.rs`
  - `/Users/tom/Dev/projects/effigy/crates/effigy-containers/src/lib.rs`
  - `/Users/tom/Dev/projects/effigy/crates/effigy-manifest/src/config_sections.rs`

## Boundaries

- Stay within the existing gateway/container architecture — don't replace or
  rewrite the reverse proxy model, just extend the route model and registration
  layer.
- Do not add a persistent system daemon / launchd plist for the gateway.
  On-demand lifecycle (starts on first `system up`, exits when last project
  deregisters) is an explicit product decision.
- Do not attempt Linux or Windows resolver integration — macOS first, same as
  `g02.014`.
- Do not make TCP service DNS names user-configurable in v1 — fixed canonical
  catalog only.
- Do not widen to shared services refcounting or garbage collection — that
  belongs to a future lane.
- Do not touch `.github/workflows/` without explicit human approval.
- Follow repo constraints from
  [`AGENTS.md`](/Users/tom/Dev/projects/effigy/AGENTS.md).

## Important Context

### Design decisions (treat these as settled)

**1. Loopback IP aliases per project (TCP service isolation)**
- Each project gets a stable, assigned IP in `127.1.0.x` space.
- Assignments are persisted in gateway state so they survive restarts.
- Docker binds TCP service ports to that specific IP:
  `-p 127.1.0.1:5432:5432`.
- Standard ports stay standard — no per-project port juggling.
- Pre-allocate a range (e.g. `127.1.0.1`–`127.1.0.50`) during
  `effigy gateway setup` (already a sudo step) so container startups never
  need elevated privileges.

**2. HTTP services: ephemeral host ports + post-startup discovery**
- HTTP container services use `0:CONTAINER_PORT` in compose (no declared host
  port).
- After `docker compose up`, effigy runs `docker compose port SERVICE PORT`
  to discover the assigned host port.
- Gateway is registered with the discovered ephemeral port.
- Developer never sees these ports — they use the `.test` domain.

**3. `<service>.<app>.test` DNS naming for TCP services**
- Format: `<service>.<app>.test` — e.g. `db.myapp.test`, `redis.myapp.test`.
- Auto-derived from manifest service declarations; no user config needed.
- All service subdomains for a project resolve to the project's loopback IP
  (e.g. `127.1.0.1`). Port differentiation happens at the client, not DNS.
- HTTP subdomain routes (e.g. `admin.myapp.test`) continue to resolve to
  `127.0.0.1` and go through the reverse proxy. Explicit HTTP routes take
  precedence over auto-derived service routes if names collide.
- Canonical service-to-prefix mapping (fixed catalog, v1):

  | Catalog | Domain prefix |
  |---|---|
  | `postgres` | `db` |
  | `mariadb` / `mysql` | `db` |
  | `redis` | `redis` |
  | `elasticsearch` | `search` |
  | `minio` / `s3` | `s3` |
  | `mail` (HTTP UI) | `mail` |
  | `mail` (SMTP) | `smtp` |

**4. Route model change: per-domain `dns_ip`**
- Add `dns_ip: Option<Ipv4Addr>` to route entries.
- DNS resolver returns `route.dns_ip` when present, falls back to
  `config.resolve_to` (`127.0.0.1`) otherwise.
- HTTP routes: `dns_ip` absent (or `127.0.0.1`), `proxy_target` present.
- TCP service routes: `dns_ip` = project loopback IP, no `proxy_target`.
- Reverse proxy only acts on routes with a `proxy_target` — ignores TCP
  routes.
- This is the foundational change everything else builds on.

**5. On-demand gateway lifecycle**
- Gateway starts when `system up` is run and no gateway is already bound on
  15353.
- Gateway exits when the last project deregisters all its routes.
- `/etc/resolver/test` stays written permanently (one-time setup). A dead port
  means `.test` DNS fails quietly — correct behaviour when no containers run.
- Stale route cleanup: on `system up`, sweep the route table for routes whose
  project paths have no running containers (via `docker compose ps`). Remove
  stale entries before registering new ones.

**6. Mail is the awkward service**
- MailHog / Mailpit has both an HTTP web UI and an SMTP TCP port.
- `mail.myapp.test` → HTTP route → `127.0.0.1` → reverse proxy → web UI.
- `smtp.myapp.test` → TCP route → `127.1.0.x:1025`.
- Two route entries, two DNS names, different route types. Treat them as
  distinct service entries from the same mail catalog.

**7. Env var injection**
- Because service names are canonical, effigy can inject env vars into the
  container automatically: `POSTGRES_HOST=db.myapp.test`,
  `REDIS_HOST=redis.myapp.test`, etc.
- These are more readable than IPs and make connection strings portable.

### Planning lineage

- `g02.014` completed the gateway foundation but explicitly deferred
  "simultaneous multi-project proof" to `g02.016`.
- `g02.016` shipped bounded coordination (auto port allocation for
  generated-compose, cross-project status) but did not solve loopback IP
  isolation or TCP service DNS.
- This design conversation fills the remaining gap those two lanes left open.

### Open tensions

- The loopback alias range pre-allocation approach (127.1.0.1–50) caps
  simultaneous projects at 50. Generous for now, but worth noting.
- Shared services (postgres running as a single shared instance) already
  exist — how does the loopback IP model interact with them? A shared postgres
  likely gets a well-known IP rather than a per-project one. Needs a decision
  early in implementation.

## Suggested Next Move

Create a new roadmap card (e.g. `g02.018`) for this work. Scope it around the
three concrete deliverables:

1. Route model: add `dns_ip` to route entries, update DNS resolver to use it.
2. Loopback IP assignment: persistent per-project allocation, gateway setup
   pre-allocates the range.
3. TCP service DNS auto-registration: derive `<service>.<app>.test` routes
   from manifest service declarations, bind to project loopback IP.

Start implementation with the route model change — it's the foundation
everything else touches. The DNS resolver change is small (~10 lines). The
registration layer change is larger but well-isolated.

Do not start implementation without a roadmap card. The current active strict
lane is `g02.007` (release prep) — confirm whether this work is in-bounds
alongside it or should queue after `v0.3` ships.

## Completion Protocol

This is a design handoff to an implementation thread — no batch card, roadmap
entry, or log update exists yet for the work itself.

1. No batch card to confirm — this is pre-implementation.
2. Create a roadmap card before execution starts.
3. The continuation envelope is fully open — no budget exhausted.
4. No pause signal fired; the pause is deliberate (design-complete,
   waiting for implementation thread).
5. Another thread genuinely needs to take over — this thread was design-only.
6. Unresolved risks: shared-services / loopback IP interaction (see Open
   tensions above); mail dual-route handling needs a concrete implementation
   decision.
7. Next task: create `g02.018` roadmap card, then implement the route model
   `dns_ip` field as the first batch.
