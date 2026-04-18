# 266 Implement Gateway Command Foundation

Status: ready
Updated: 2026-04-18
Roadmap: `g02.014`
Spec: `docs/specs/014-rust-native-gateway-strict-lane.md`

## Objective

Make the gateway product-real as a host-native command surface before wiring
project route registration into it.

## Scope

- add `effigy gateway` CLI parsing, help, JSON participation, and runner
  dispatch
- implement `gateway up`, `gateway down`, and `gateway status` on top of
  `effigy-gateway`
- define the standard gateway state path under `~/.effigy/gateway`
- project lifecycle/status output into user-facing plain and JSON surfaces
- wire macOS resolver setup/teardown hooks with clear guidance on permission
  failure

## Out Of Scope

- manifest `dns.domain` integration
- route registration on `container up` / `container down`
- real-project proof of hostname routing
- TLS setup and certificate generation

## Acceptance

- `effigy gateway up`, `down`, and `status` parse and render help correctly
- the runner can start and stop the host-native gateway daemon
- status reports the live gateway process and current route-table state
- resolver setup/teardown failures surface as clear product guidance instead of
  silent no-op behavior
- the batch leaves route registration as the explicit next integration step

## Next Task

Implement now. If this lands cleanly, the next ready batch should wire
manifest DNS and route registration through the container lifecycle.
