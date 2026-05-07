# 534 - Extract Runtime DNS Policy Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move runtime DNS override planning and rendering out of
`crates/effigy-containers/src/lib.rs` into a focused runtime module without
changing generated compose behavior.

## Scope

- create `crates/effigy-containers/src/runtime/dns.rs`
- move runtime DNS helpers where dependencies remain clean:
  - `RuntimeDnsOverrideRoutes`
  - `runtime_route_domains`
  - `base_domain_from_route`
  - `materialize_runtime_dns_override`
  - `collect_compose_service_names`
  - `resolve_runtime_dns_servers`
  - `resolve_runtime_gateway_address`
  - `colima_home_dir`
  - `render_runtime_dns_override`
- keep current callers working through crate-local imports or stable exports
- preserve rendered runtime DNS compose output and error text

## Non-Goals

- no eject split
- no policy loading split
- no workspace module split
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runtime DNS override logic lives outside `lib.rs`,
generated compose DNS tests pass, and public callers still compile.

## Closeout

Runtime DNS override logic now lives under
`crates/effigy-containers/src/runtime/dns.rs`. `lib.rs` dropped from 885 to 641
lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-dns-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-dns-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-dns-test cargo test -p effigy-containers -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`535-extract-generated-compose-eject-module.md`](./535-extract-generated-compose-eject-module.md).
