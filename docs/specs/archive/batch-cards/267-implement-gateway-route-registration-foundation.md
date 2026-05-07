# 267 Implement Gateway Route Registration Foundation

Status: landed
Updated: 2026-04-18
Roadmap: `g02.014`
Spec: `docs/specs/014-rust-native-gateway-strict-lane.md`

## Objective

Make the gateway see real project routes by wiring manifest DNS declaration and
container lifecycle registration into the product path.

## Scope

- add manifest DNS contract support for project domain declaration
- validate the DNS surface through schema/doctor
- register routes on container bring-up and deregister them on container
  shutdown
- connect route target resolution to the existing container/service ownership
  surface
- project registration results through user-facing command output where needed

## Out Of Scope

- multi-project gateway dashboard work owned by `g02.016`
- HTTPS certificate generation and TLS setup flow
- real-project hostname proof beyond the one bounded loop needed to validate
  registration wiring

## Acceptance

- a project can declare its gateway domain through the manifest
- container lifecycle writes and removes gateway routes through the shared route
  table
- route-registration failures surface as clear product errors or warnings
- one bounded proof shows a manifest-owned route entering and leaving the route
  table through Effigy itself

## Landed Outcome

- `[containers.<name>.dns]` is now a real manifest surface with schema/doctor
  validation
- `effigy container up`, `down`, and `reset` now register or remove gateway
  routes when a container declares a DNS domain
- attached owner-exit shutdown now removes the route alongside container
  teardown
- route registration now fails clearly when a DNS-enabled container does not
  declare a usable host port, and runner tests cover the route-table roundtrip
  through the new product helper

## Next Task

No further execution lives on this card. The next batch should prove the plain
HTTP hostname loop in one real project and harden whatever that proof exposes.
