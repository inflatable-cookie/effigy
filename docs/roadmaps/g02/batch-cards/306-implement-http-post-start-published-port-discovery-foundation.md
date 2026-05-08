# 306 Implement HTTP Post-Start Published-Port Discovery Foundation

Status: archived
Updated: 2026-04-22
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the next bounded `g02.020` slice by removing manifest-declared host-port
requirements from HTTP gateway registration on the current shipped container
path.

## In Scope

- discover effective published HTTP host ports after container startup on the
  current bounded runtime path
- thread that runtime port discovery through HTTP gateway registration
- keep existing route validation honest against the discovered runtime binding
- refresh product-facing tests or docs that currently imply gateway
  registration depends on declared host ports

## Out Of Scope

- TCP service DNS alias derivation
- loopback-IP-backed TCP route registration
- shared-service DNS reuse or env-var injection
- broader container registration rewrites beyond what HTTP discovery needs

## Acceptance Criteria

- HTTP gateway registration no longer requires manifest-declared host ports on
  the generated-compose path
- discovered host ports come from running container runtime data, not static
  manifest assumptions
- the route-validation path still refuses unrelated listener collisions
- tests make the post-start discovery contract clear enough for later TCP
  service registration work

## Validation

- `cargo test -p effigy --lib gateway_command`
- `cargo test -p effigy --lib container_command`
- `cargo check -p effigy --lib --tests`
- `git diff --check`

## Result

Landed. HTTP gateway registration now resolves its target from running
container published-port data first, so generated-compose services can use
post-start host-port discovery instead of relying on manifest-declared host
ports during registration.

This batch kept the validation seam honest by still checking the selected
target against the running project rows and unrelated host listeners, while
adding focused tests for ephemeral and service-specific runtime bindings.

## Next Task

Execute `307` to start bounded TCP-service DNS alias registration on the
loopback-IP foundation.
