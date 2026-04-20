# 303 Implement Loopback-IP Allocation And Gateway Setup Foundation

Status: staged
Updated: 2026-04-20
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the second bounded `g02.020` slice by making the gateway own stable
loopback-IP allocation for project and shared-service route groups on the
bounded macOS path.

## In Scope

- add persistent loopback-IP allocation in `127.1.0.x` space for gateway-owned
  project or shared-service identities
- thread that allocation through gateway state and tests so assignments survive
  restart
- extend gateway setup to provision the bounded alias range during the
  existing elevated setup step
- keep the current on-demand gateway lifecycle intact
- refresh product-facing tests or docs that need to describe the setup/state
  contract honestly

## Out Of Scope

- HTTP post-start published-port discovery
- manifest-driven TCP service alias derivation or registration
- shared-service DNS reuse beyond the common allocator contract
- env-var injection or broader container registration rewrites

## Acceptance Criteria

- the gateway can persist and reload stable loopback-IP assignments
- setup owns alias-range provisioning on the bounded macOS path
- assignments are available for later route registration without requiring
  elevated privileges during normal runtime
- the current gateway lifecycle and existing HTTP route behavior stay intact
- tests make the allocation and persistence contract clear enough for later
  TCP-service registration work

## Validation

- `cargo test -p effigy-gateway --lib`
- `cargo check -p effigy --lib --tests`
- `git diff --check`

## Result

Staged. This card is the intended second `g02.020` execution move now that the
route-model seam from `301` is real.

## Next Task

Execute this card when `g02.020` resumes after the deliberate `g02.007` and
`g02.019` fronts.
