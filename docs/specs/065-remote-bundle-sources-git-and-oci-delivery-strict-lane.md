# 065 - Remote Bundle Sources: Git And OCI Delivery Strict Lane

Roadmap: [`g04.022`](../roadmaps/g04/022-remote-bundle-sources-git-and-oci-delivery.md)

Status: Active
Owner: Platform
Created: 2026-05-10

## Purpose

Replace the legacy bundle `base`/`base_path` split with one typed remote/local
source model and land the first git/OCI delivery path behind the existing
bundle materialization boundary.

This lane owns:

- unified `[bundle].base` parsing
- `base_path` removal
- shared resolved bundle-source materialization
- git and OCI source resolution, cache identity, and stale detection
- `bundle sync`
- source-aware `bundle inspect`

## Hard Boundaries

- do not widen into deployment/provider workflows
- do not add bundle publish/push behavior
- keep refresh explicit; no background polling
- keep downstream bundle loading consuming one shared `local_path`
- no `.github/workflows/` edits
- no release execution

## Current Ready Card

- [`631-add-oci-bundle-source-resolver-and-cache-identity.md`](../roadmaps/g04/batch-cards/631-add-oci-bundle-source-resolver-and-cache-identity.md)

## Execution Chain

- `626` complete: opened the lane, promoted the remote bundle-source contract
  anchor, and selected the first config-boundary card
- `627` complete: locked the unified `base` grammar, `base_path` removal
  behavior, source taxonomy, shared materialization boundary, and first-round
  git/OCI cache/update rules in the active contract
- `628` complete: landed the typed bundle-source manifest model, kept string
  sugar plus legacy shipped `name`, removed `base_path`, and updated the
  direct config/export surfaces around the new `path` block form
- `629` complete: extracted shared shipped/path source materialization and one
  shared defaults-loading seam on top of the materialized source result
- `630` complete: landed the first git bundle-source resolver, stable git cache
  identity rules, and local git proof coverage
- `631` ready: add the OCI bundle-source resolver and shared cache identity
  rules before the later sync/inspect surface batches

## Exit Condition

This lane is complete when Effigy can resolve shipped, path, git, and OCI
bundle sources through one stable source model, inspect/sync remote bundles,
and no longer accepts `base_path`.

## Next Task

Execute
[`631-add-oci-bundle-source-resolver-and-cache-identity.md`](../roadmaps/g04/batch-cards/631-add-oci-bundle-source-resolver-and-cache-identity.md).
