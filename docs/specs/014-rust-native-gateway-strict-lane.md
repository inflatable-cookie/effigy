# 014 Rust-Native Gateway Strict Lane

Status: active
Updated: 2026-04-18
Roadmap: `g02.014`

## Context

The `effigy-gateway` crate is already real, but the product surface is still
crate-only. The repo cannot treat gateway work as execution-ready until the
host-native command surface, lifecycle ownership, and route-registration path
exist in the root product.

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

`strict-ready`

The crate-first groundwork is already shipped:

- DNS resolver and reverse proxy live in `crates/effigy-gateway`
- route-table load/store and route registration helpers are real
- gateway server lifecycle and PID-file handling are real
- macOS resolver setup helpers are real
- TLS and certificate helpers are real

What is still missing is product wiring through CLI, runner, manifest, and
container lifecycle.

## Integration Constraint

This lane should land in bounded product batches rather than one broad gateway
rewrite:

- make the host-native command surface real first
- wire route registration only after lifecycle/status behavior is trustworthy
- prove the plain HTTP route loop before widening into TLS closeout
- keep the eventual route dashboard split clean with `g02.016`

## Remaining Integration Work

The bounded continuation runway is:

1. `266` — host-native gateway command foundation: CLI/help/dispatch, daemon
   lifecycle, status projection, JSON output, and resolver setup/teardown
   hooks with clear product guidance
2. planned next — manifest DNS contract plus route registration on container
   up/down so the gateway sees real project routes
3. planned next — one real-project proof for the plain HTTP gateway loop and
   hardening from that proof
4. planning checkpoint — finish TLS setup/cert flow here, then leave the fuller
   cross-project dashboard work to `g02.016`

## Exit Condition

This strict lane is complete when:

- `effigy gateway up/down/status` are real product commands
- manifest DNS configuration and route registration are wired through the
  product path
- one real project proves the non-TLS `.test` loop end to end
- the remaining TLS work is either landed or cleanly bounded as the final
  in-lane follow-up

## Next Task

Execute `266` now. Make the host-native gateway command surface real without
pulling route registration into the same batch.
