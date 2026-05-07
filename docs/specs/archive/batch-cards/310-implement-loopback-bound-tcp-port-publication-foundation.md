# 310 Implement Loopback-Bound TCP Port Publication Foundation

Status: landed
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

Landed. The generated-compose rewrite now emits loopback-bound TCP service
ports on the product path, and the host gateway now carries the bounded
fallback the live Colima/nerdctl path needed:

- local project-owned TCP alias ports are rewritten onto `127.1.0.1`
- project-owned alias services also keep one dynamic runtime host binding so
  the host still has a reachable upstream when Colima refuses the direct
  loopback-bound publication
- shared-service compose keeps its dynamic host binding while adding the
  loopback-bound standard port
- gateway registration accepts the new `127.1.0.1:<host>:<container>` port
  syntax during compose-file inspection
- DNS-only service routes now persist `tcp_port` plus runtime-discovered
  `tcp_target` metadata in the gateway route table
- the host gateway daemon now owns bounded TCP listeners on the assigned
  `127.1.x.x:<service-port>` address and forwards them to the runtime
  published port

Live proof after restarting the gateway on the new binary:

- `db.acme.test`, `smtp.acme.test`, and `s3.acme.test` all resolve to
  `127.1.0.1`
- direct host connections to `127.1.0.1:5432`, `127.1.0.1:1025`, and
  `127.1.0.1:9000` now succeed on the bounded macOS path
- `minio.acme.test` still registers to the real HTTP console target instead of
  the S3 TCP alias port
- the `underlay-reference` generated path now shows the honest split:
  loopback-bound alias port plus dynamic runtime host binding where the host
  gateway needs one

## Next Task

Resume `309` and finish the remaining real-project proof work now that the
runtime publication gap is closed.
