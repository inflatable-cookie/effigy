# 269 Plan Gateway TLS Closeout Batch

Status: archived
Updated: 2026-04-18
Roadmap: `g02.014`
Spec: `docs/specs/014-rust-native-gateway-strict-lane.md`

## Objective

Turn the remaining gateway TLS work into one bounded final batch instead of
letting `g02.014` drift after the plain HTTP proof.

## Scope

- assess what is already real in `effigy-gateway` for certificates and HTTPS
- define the smallest product batch needed for `effigy gateway setup-tls` and
  route-owned certificate use
- record any host prerequisites that stay operator-owned rather than product
  bugs
- leave the lane with one explicit implementation card or an explicit closeout
  call if TLS is smaller than expected

## Out Of Scope

- implementing the TLS batch itself
- multi-project coordination or gateway dashboard work
- widening into non-container task-owned routes

## Acceptance

- the remaining TLS work is described on a trustworthy product boundary
- one explicit next execution card exists, or the lane closes with a concrete
  reason
- the front-door planning surfaces stop advertising `268` as the next move

## Decision

`g02.014` is not done enough to close.

What is already real:

- `effigy-gateway` already has mkcert CA checks/install helpers
- the crate can generate and load per-domain cert/key pairs under the gateway
  cert directory
- the HTTPS proxy path and SNI-based certificate resolver already exist in the
  crate
- manifest and route registration already carry `tls = true` through the
  product path

What is still missing in the product:

- no `effigy gateway setup-tls` CLI or runner surface
- no product-owned certificate generation when a TLS route is registered
- no product-owned certificate cleanup when a TLS route is removed
- no bounded real-project proof for the HTTPS hostname loop
- no operator-facing status/help that makes the mkcert prerequisite explicit

Operator-owned prerequisites that stay out of product scope:

- macOS `/etc/resolver/test` still needs host-level write access on this
  machine
- mkcert trust-store installation remains a host operation that may prompt for
  sudo or platform trust access

## Result

The smallest trustworthy closeout is one final execution batch:

- add `effigy gateway setup-tls`
- generate route-owned certs for TLS-enabled container domains
- keep HTTPS startup and status output honest when certs or CA setup are
  missing
- prove one real TLS loop on a consumer repo

That batch is now explicit as card `270`.

## Next Task

No further execution lives on this card. Execute `270` to finish the remaining
TLS work for `g02.014`.
