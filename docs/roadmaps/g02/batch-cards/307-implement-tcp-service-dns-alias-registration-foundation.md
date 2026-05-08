# 307 Implement TCP-Service DNS Alias Registration Foundation

Status: archived
Updated: 2026-04-22
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the next bounded `g02.020` slice by registering DNS-only TCP service
aliases from the shipped container/catalog shape onto the loopback-IP
foundation already in gateway state.

## In Scope

- derive bounded TCP service aliases from the current shipped service catalog
  and container policy shape
- register DNS-only gateway routes with `dns_ip` and no HTTP proxy target
- reuse the persisted loopback-IP registry for project-owned service groups
- keep the current HTTP route registration path intact
- refresh product-facing tests or docs that need to describe the DNS-only
  service-route contract honestly

## Out Of Scope

- shared-service DNS reuse across several consuming projects
- env-var injection for service hostnames
- mail dual-route widening beyond whatever is needed to keep the bounded
  service alias model honest
- broader manifest schema redesigns

## Acceptance Criteria

- project-owned TCP services can register stable `.test` DNS aliases without
  going through the HTTP proxy
- registered TCP aliases resolve to the persisted project loopback IP
- ordinary HTTP gateway registration continues to work on the post-start
  published-port discovery path
- tests make the DNS-only route contract clear enough for later shared-service
  reuse work

## Validation

- `cargo test -p effigy --lib gateway_registration -- --nocapture`
- `cargo test -p effigy-gateway --lib`
- `cargo check -p effigy --lib --tests`
- `git diff --check`

## Result

Landed. Container gateway registration now derives bounded DNS-only TCP
service aliases such as `db.<app>.test` from the shipped service catalog
shape, persists and reuses one loopback IP per project through gateway state,
and leaves explicit manifest-owned HTTP route domains in control when names
collide.

This batch also made the route model honest for DNS-only entries by letting
proxy routes keep an upstream target while TCP aliases resolve through `dns_ip`
without pretending to be HTTP upstreams.

## Next Task

Execute `308` to make bounded shared services fit the same DNS alias model
through shared-service loopback reuse.
