# 308 Implement Shared-Service DNS Reuse Foundation

Status: archived
Updated: 2026-04-22
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the next bounded `g02.020` slice by making bounded shared services fit
the new TCP-service DNS model honestly instead of stopping at project-owned
loopback-IP aliases.

## In Scope

- derive shared-service DNS aliases on the existing bounded shared-service path
- let several project-facing aliases resolve to one shared-service loopback IP
  when they reuse the same shared backing service
- keep the current project-owned TCP alias path intact
- refresh product-facing tests or docs that need to describe shared-service
  DNS reuse honestly

## Out Of Scope

- broader shared-service lifecycle redesign or garbage collection
- env-var injection widening beyond what shared-service alias registration
  immediately needs
- Linux or Windows resolver work
- broader manifest schema changes

## Acceptance Criteria

- shared-service consumers can resolve stable project-facing aliases without
  duplicating one shared backing service onto several project IPs
- project-owned and shared-service aliases can coexist in the same bounded DNS
  model without breaking HTTP proxy routes
- tests make the shared-service reuse contract clear enough for later
  consumer-repo migration work

## Validation

- `cargo test -p effigy --lib gateway_registration -- --nocapture`
- `cargo test -p effigy-gateway --lib`
- `cargo check -p effigy --lib --tests`
- `git diff --check`

## Result

Landed. Container gateway registration now derives bounded shared-service DNS
aliases from `policy.shared_services`, maps them onto one shared loopback-IP
identity per shared backing project, and keeps those DNS-only routes honest by
skipping any hostname already owned by an explicit manifest route or a
project-owned service alias.

This keeps the shipped alias model compatible across several consuming
projects without pretending each shared backing service needs a separate
per-project loopback assignment.

## Next Task

Execute `309` to prove the new TCP service DNS model in one real consumer repo
and migrate that repo off hardcoded local service ports where the shipped
aliases now cover the runtime path.
