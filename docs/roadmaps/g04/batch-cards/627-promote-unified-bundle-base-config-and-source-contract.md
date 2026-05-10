# 627 - Promote Unified Bundle Base Config And Source Contract

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

## Goal

Lock the first contract boundary for `g04.022` before parser and resolver work
starts.

## Scope

- finalize the unified `[bundle].base` grammar
- lock the `base_path` removal error and migration wording
- lock the first typed source taxonomy
- lock the shared `ResolvedBundleSource` materialization result
- lock first-round git and OCI cache/update rules
- define the bounded `bundle inspect` and `bundle sync` minimum metadata

## Acceptance

- the contract names all accepted `base` forms
- `base_path` removal behavior is explicit and migration-safe
- all source types converge on one materialization boundary
- git and OCI stale/update detection rules are explicit
- first-round inspect/sync scope is bounded and non-ambiguous
- the roadmap/spec docs point at this card as the ready next move

## Closeout

The active contract now locks:

- the unified string-plus-block `base` grammar
- the `base_path` removal rule and migration error
- the first typed source taxonomy
- the shared `ResolvedBundleSource` materialization result
- the bounded git/OCI cache and stale-detection rules

## Next Task

Execute
[`628-add-unified-bundle-base-model-and-base-path-removal.md`](./628-add-unified-bundle-base-model-and-base-path-removal.md).
