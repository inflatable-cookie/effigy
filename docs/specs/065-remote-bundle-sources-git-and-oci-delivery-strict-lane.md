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

- [`628-add-unified-bundle-base-model-and-base-path-removal.md`](../roadmaps/g04/batch-cards/628-add-unified-bundle-base-model-and-base-path-removal.md)

## Execution Chain

- `626` complete: opened the lane, promoted the remote bundle-source contract
  anchor, and selected the first config-boundary card
- `627` complete: locked the unified `base` grammar, `base_path` removal
  behavior, source taxonomy, shared materialization boundary, and first-round
  git/OCI cache/update rules in the active contract
- `628` ready: land the typed bundle-source manifest model and remove
  `base_path` before resolver work starts

## Exit Condition

This lane is complete when Effigy can resolve shipped, path, git, and OCI
bundle sources through one stable source model, inspect/sync remote bundles,
and no longer accepts `base_path`.

## Next Task

Execute
[`628-add-unified-bundle-base-model-and-base-path-removal.md`](../roadmaps/g04/batch-cards/628-add-unified-bundle-base-model-and-base-path-removal.md).
