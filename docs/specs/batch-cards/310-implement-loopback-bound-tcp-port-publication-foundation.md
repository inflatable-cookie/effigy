# 310 Implement Loopback-Bound TCP Port Publication Foundation

Status: next
Updated: 2026-04-22
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the proof-exposed `g02.020` follow-through by publishing shipped TCP
service ports onto the assigned loopback IP and standard service port on the
generated-compose path, so DNS-only aliases resolve to a listener that is
actually there.

## In Scope

- update generated-compose port policy so shipped TCP service aliases bind onto
  the assigned loopback IP and standard service port instead of only onto
  auto-allocated localhost ports
- keep HTTP route publication on the current live published-port discovery
  path
- keep per-project and shared-service loopback identity reuse aligned with the
  existing gateway route registration model
- add focused tests that prove the compose/runtime policy now matches the
  DNS-only alias contract

## Out Of Scope

- new alias categories beyond the shipped catalog contract
- Linux or Windows resolver work
- broader consumer-repo migration beyond resuming `309` after this lands
- unrelated container lifecycle redesign

## Acceptance Criteria

- generated-compose TCP services published through the shipped alias contract
  listen on the assigned loopback IP and their standard service port
- project-owned and shared-service DNS-only aliases resolve to a reachable
  listener on the actual product path
- HTTP route registration remains on auto-allocated published ports where
  appropriate
- tests make the port-publication contract clear enough to resume the
  `underlay-reference` proof immediately after landing

## Validation

- `cargo test -p effigy-containers --lib`
- `cargo test -p effigy --lib gateway_registration -- --nocapture`
- `cargo test -p effigy-gateway --lib`
- `cargo check -p effigy --lib --tests`
- `git diff --check`

## Result

Next. `309` proved that route registration and DNS answers are now real, but
also exposed that generated compose still publishes TCP listeners on
auto-allocated localhost ports instead of the assigned loopback IP.

## Next Task

Execute this card in `crates/effigy-containers/src/policy_support.rs` first,
then resume `309` in `/Users/tom/Dev/projects/underlay-reference`.
