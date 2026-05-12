# 014 Rust-Native Gateway Strict Lane

Status: complete
Updated: 2026-04-18
Roadmap: `g02.014`

## Context

The bounded gateway product lane is now real. The root product owns command
integration, route lifecycle, and real consumer proofs for both plain HTTP and
HTTPS on the intended boundary.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/014-rust-native-gateway.md`
- `docs/architecture/020-container-infrastructure-design.md`

## Lane Focus

This lane owns:

- host-native `effigy gateway` command integration
- gateway lifecycle and status projection in the runner
- manifest DNS contract and route-registration wiring
- one real-project proof of the plain HTTP gateway loop
- bounded follow-through on TLS setup once the non-TLS loop is real

## Current Posture

`complete`

The crate-first groundwork is already shipped:

- DNS resolver and reverse proxy live in `crates/effigy-gateway`
- route-table load/store and route registration helpers are real
- gateway server lifecycle and PID-file handling are real
- macOS resolver setup helpers are real
- TLS and certificate helpers are real

The product wiring is now real through CLI, runner, manifest, and container
lifecycle.

## Integration Constraint

This lane should land in bounded product batches rather than one broad gateway
rewrite:

- make the host-native command surface real first
- wire route registration only after lifecycle/status behavior is trustworthy
- prove the plain HTTP route loop before widening into TLS closeout
- keep the eventual route dashboard split clean with `g02.016`

## Landed Outcome

The bounded continuation runway is complete through:

1. `266` — host-native gateway command foundation: CLI/help/dispatch, daemon
   lifecycle, status projection, JSON output, and resolver setup/teardown
   hooks with clear product guidance
2. `267` — manifest DNS contract plus route registration on container up/down
   so the gateway sees real project routes
3. `268` — one real-project proof for the plain HTTP gateway loop and bounded
   hardening from that proof
4. `269` — plan the final TLS closeout batch now that the non-TLS loop is
   proven on a real project
5. `270` — final in-lane execution: `gateway setup-tls`, route-owned cert
   generation, honest HTTPS readiness/status, and one real TLS proof before
   leaving broader coordination to `g02.016`

What is real in the product path:

- `effigy gateway up`, `down`, and `status` are wired through CLI help, JSON
  mode, labels, and runner dispatch
- the root product owns a hidden gateway daemon entrypoint and detached spawn
  path
- daemon startup failures now report their real bind/startup cause
- default privileged startup fails fast with explicit permission guidance
- unprivileged override ports prove the full `up` / `status` / `down` loop on
  a real machine even when resolver setup still needs sudo
- containers can declare `[containers.<name>.dns]`, pass schema/doctor
  validation, and register or remove gateway routes on `up` / `down` / `reset`
- attached owner-exit shutdown removes the registered route alongside container
  teardown
- one real consumer proof now shows the plain HTTP hostname loop through the
  shared gateway path, with one in-batch hardening fix for multi-port stacks:
  `[containers.<name>.dns].port`
- the gateway now serves HTTPS on the product path with route-owned cert
  generation, cert cleanup, live cert reload, and `setup-tls` guidance
- one real consumer proof now shows the HTTPS hostname loop through the shared
  gateway path, while keeping resolver setup and trust-store install as honest
  operator-owned prerequisites on this machine

## Exit Condition

This strict lane is complete because:

- `effigy gateway up/down/status` are real product commands
- manifest DNS configuration and route registration are wired through the
  product path
- one real project proves the non-TLS `.test` loop end to end
- the remaining TLS work is now landed on the final in-lane follow-up

## Next Task

No further execution lives in this strict lane. Any broader cross-project
status/dashboard work belongs to `g02.016`.
