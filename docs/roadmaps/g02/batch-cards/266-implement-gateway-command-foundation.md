# 266 Implement Gateway Command Foundation

Status: archived
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

## Landed Outcome

- `effigy gateway up`, `down`, and `status` are now real CLI and runner
  surfaces with JSON/plain output
- the root product owns a hidden gateway daemon entrypoint and detached spawn
  path for the host-native gateway process
- early daemon failures now report the real bind/startup error instead of a
  fake clean exit
- resolver setup/teardown failures surface as warnings or clear operator-facing
  guidance instead of silent no-op behavior
- one real machine proved the full `up` / `status` / `down` lifecycle with
  unprivileged override ports, while the default privileged path now fails fast
  with an explicit permission requirement

## Next Task

No further execution lives on this card. Wire manifest DNS and route
registration through the container lifecycle next.
