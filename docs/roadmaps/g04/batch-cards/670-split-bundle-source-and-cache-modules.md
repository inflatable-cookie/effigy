# 670 - Split Bundle Source And Cache Modules

Roadmap: [`../036-manifest-section-decomposition.md`](../036-manifest-section-decomposition.md)
Strict lane: [`../../../specs/072-manifest-section-decomposition-strict-lane.md`](../../../specs/072-manifest-section-decomposition-strict-lane.md)
Contract: [`../../../contracts/028-manifest-section-decomposition-contract.md`](../../../contracts/028-manifest-section-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move bundle source materialization and cache behavior out of `bundles.rs`
behind a section-owned internal module.

## Scope

- extract git/OCI/path bundle source resolution helpers
- extract bundle cache root, cache identity, stale detection, and refresh
  helpers
- keep `sync_bundle_source`, `inspect_bundle_source`,
  `apply_bundle_defaults`, and public re-exports behavior-compatible
- keep existing TOML grammar and error text stable
- move or preserve the focused git/OCI source tests with the new owner

## Non-Goals

- no bundle descriptor/input/template rewrite
- no manifest composition changes
- no new bundle source types
- no public API break
- no command behavior changes

## Acceptance

- `bundles.rs` no longer owns remote source/cache implementation details
- the new source/cache module has a clear narrow boundary
- git, OCI, and path bundle behavior remains unchanged
- representative bundle fixture tests still pass
- god-file pressure is reduced without adding vague utility modules

## Outcome

- added `crates/effigy-manifest/src/bundles/source.rs`
- moved bundle source selection, git/OCI materialization, cache identity, sync,
  inspect, and focused source tests into the new owner
- kept `bundles.rs` as the bundle descriptor/default/template owner and public
  facade for bundle APIs
- reduced `bundles.rs` from 2,060 lines to 856 lines

## Validation

```sh
cargo test -p effigy-manifest
cargo check --bin effigy
git diff --check
```

Optional if local config has a bundle fixture available:

```sh
effigy bundle inspect --json
```

## Next Task

Execute `671` for the next manifest section split.
