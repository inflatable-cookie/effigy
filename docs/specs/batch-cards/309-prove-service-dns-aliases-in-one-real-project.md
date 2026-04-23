# 309 Prove Service DNS Aliases In One Real Project

Status: landed
Updated: 2026-04-23
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the next bounded `g02.020` slice by proving the shipped HTTP and TCP
service DNS model in one real consumer repo instead of stopping at library and
runner tests inside Effigy alone.

## In Scope

- migrate one real consumer repo onto the shipped `.test` alias model where it
  still hardcodes local service ports
- prove project-owned and shared-service aliases on the actual product path
- capture any bounded proof-exposed fixes needed to keep the shipped route and
  DNS model honest
- refresh lane-facing docs once the proof is trustworthy

## Out Of Scope

- broad migration across every consumer repo
- new alias categories or manifest-surface widening beyond proof-exposed fixes
- Linux or Windows resolver work
- unrelated local-network redesign

## Acceptance Criteria

- one real consumer repo can use shipped `.test` HTTP and TCP service names
  without depending on hardcoded local service ports
- the proof exercises both project-owned and shared-service alias behavior on
  the intended product path
- any proof-exposed fix stays bounded to making the shipped contract honest
- docs leave the lane with a truthful next continuation after the proof

## Validation

- proof commands and repo-local validation captured in the batch result
- `git diff --check`

## Result

Landed. The real-project proof now covers both sides of the shipped contract:

- `/Users/tom/Dev/projects/underlay-reference` proves the project-owned path
  on generated services
- `/Users/tom/Dev/legacy/sites/contactpatch` proves the shared-service path
  on a bundle-driven generated stack
- migrating a repo off its hand-written `compose_file` and onto bundled
  generated services is feasible on the shipped path
- HTTP route registration still works honestly on the generated path, with
  `acme.test`, `admin.acme.test`, `api.acme.test`,
  `contact-patch.legacy.test`, and `pma.contact-patch.legacy.test`
  all re-registered against live runtime published ports
- the current lane code derives and registers DNS-only TCP aliases for both
  project-owned and shared-service stacks, and those aliases are now reachable
  on the host through the shipped gateway fallback path
- `underlay-reference/acme-api/effigy.toml` now wires API and jobs runtime
  config through app-owned `.env` / `config/local.toml`; the task file is
  plain orchestration again, and the app runtime uses `db.acme.test`,
  `smtp.acme.test`, and `s3.acme.test` instead of internal container
  hostnames
- `underlay-reference` now uses `[bundle].base = "underlay"` and carries no
  root or `infra/dev` compose/Dockerfile runtime ownership; generated bundle
  compose is the only stack source
- `contactpatch/config/DataConnections.php` now honors injected `DB_HOST` /
  `DB_PORT` when the bundle-injected MariaDB service is flipped onto the
  shared backing-service path
- the repo README now documents the same alias-first runtime wiring instead of
  the old direct `postgres` / `mailpit` / `minio` container names
- multi-label project hosts now keep their full alias domain shape
  (`db.contact-patch.legacy.test`, not `db.legacy.test`)
- gateway registration now prunes stale container routes for a project before
  writing the current route set, so proof reruns do not leave orphaned old
  alias domains behind
- 2026-04-23 reproved the bundle-backed `underlay-reference` path:
  `effigy container up --detach --repo /Users/tom/Dev/projects/underlay-reference`
  reported all HTTP routes and installed `db/smtp/s3` container TCP aliases;
  `effigy container status --repo /Users/tom/Dev/projects/underlay-reference`
  showed all four services running under project `underlay-reference-dev`;
  `effigy gateway status --json` showed `db.acme.test:5432`,
  `smtp.acme.test:1025`, and `s3.acme.test:9000` registered on `127.1.0.1`
  with runtime TCP targets; direct workspace-container probes resolved all
  three aliases and opened all three TCP ports; `acme-api/db:migrate`
  completed successfully.

## Next Task

No further execution on this card. Continue planning outside `g02.020` or pick
up the next queued roadmap lane.
