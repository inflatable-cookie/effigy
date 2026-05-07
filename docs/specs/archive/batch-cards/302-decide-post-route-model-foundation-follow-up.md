# 302 Decide Post Route-Model Foundation Follow-Up

Status: landed
Updated: 2026-04-20
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Choose the next bounded `g02.020` batch now that the gateway route model can
carry route-specific DNS IP targets without changing the shipped HTTP proxy
path.

## Scope

- assess the remaining `g02.020` gaps against the landed `dns_ip` foundation
- decide whether loopback-IP allocation or HTTP post-start published-port
  discovery should come second
- refresh the front-door planning surfaces so `continue` resolves past `301`
  cleanly

## Out Of Scope

- implementing the follow-up batch itself
- widening into TCP service alias derivation or shared-service integration yet
- reopening release-prep or `g02.019`

## Acceptance

- one explicit next execution card exists for `g02.020`
- the chosen batch stays bounded on the landed route-model seam
- the lane front doors stop pointing at `301`

## Decision

The next `g02.020` batch should be loopback-IP allocation and gateway setup
integration, not HTTP post-start published-port discovery.

Why loopback IPs come next:

- `301` solved the foundational route-model gap. The route table can now
  express DNS answers that differ from the global gateway default, which is
  the core seam loopback-IP routing needs
- the larger unresolved product break is still TCP service identity and
  isolation across many projects. That problem cannot move without stable
  project or shared-service IP assignment
- HTTP post-start port discovery improves ergonomics, but it still only helps
  HTTP routes. Loopback allocation unlocks the broader multi-project network
  model the roadmap is actually trying to reach
- gateway setup is already the bounded elevated step in the shipped product
  path, so alias-range provisioning fits an existing operator surface instead
  of forcing new runtime privilege behavior into container startup
- once loopback assignment is real, later TCP route derivation and shared
  service reuse can build on one stable address-allocation contract instead of
  inventing it ad hoc during registration work

What stays out of the next batch:

- HTTP host-port discovery after `docker compose up`
- manifest-driven TCP alias registration
- env-var injection or shared-service DNS reuse

## Result

The next explicit `g02.020` execution batch is now card `303`.

## Next Task

Execute `303` to land loopback-IP allocation and gateway setup integration for
the bounded macOS path.
