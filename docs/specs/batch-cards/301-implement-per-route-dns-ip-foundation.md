# 301 Implement Per-Route DNS IP Foundation

Status: staged
Updated: 2026-04-20
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the first bounded `g02.020` slice by teaching the gateway route model to
carry a per-route DNS target without changing the shipped HTTP proxy contract.

## In Scope

- add `dns_ip: Option<Ipv4Addr>` to the gateway route model
- update route serialization and deserialization coverage
- make DNS responses prefer `route.dns_ip` and fall back to the gateway-wide
  default when absent
- keep proxy behavior unchanged for ordinary HTTP routes with
  `proxy_target`
- refresh any product-facing tests or docs that need to describe the new route
  shape honestly

## Out Of Scope

- loopback-IP allocation or persistence
- gateway setup alias provisioning
- HTTP post-start published-port discovery
- TCP service route derivation from manifests or catalogs
- shared-service registration changes beyond whatever is needed to keep the
  route model honest

## Acceptance Criteria

- route entries can persist an optional `dns_ip`
- DNS resolution returns the route-specific IPv4 target when present
- routes without `dns_ip` still resolve through the existing default path
- proxy-only behavior for HTTP routes remains intact
- tests prove the mixed route table behavior clearly enough that later TCP
  route work has a stable base

## Validation

- `cargo test -p effigy-gateway --lib`
- `cargo test -p effigy-gateway --test integration`
- `git diff --check`

## Result

Staged. This card exists so `g02.020` can resume on one explicit execution
move instead of reopening the design handoff each time.

## Next Task

Execute this card when `g02.020` becomes the active resumed lane.
