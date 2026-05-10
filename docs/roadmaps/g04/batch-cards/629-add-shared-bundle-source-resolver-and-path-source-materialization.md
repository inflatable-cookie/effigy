# 629 - Add Shared Bundle Source Resolver And Path Source Materialization

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

## Goal

Move bundle source handling behind one shared resolver boundary before git and
OCI sources land.

## Scope

- extract shipped and path source handling behind one shared resolver seam
- introduce the first `ResolvedBundleSource` materialization shape in code
- keep current shipped and path behavior stable
- make `apply_bundle_defaults` consume the shared materialized local-path
  result instead of source-specific branches

## Acceptance

- shipped and path sources resolve through one shared source-materialization
  boundary
- current shipped and local path bundles still load unchanged
- downstream defaults loading still consumes one canonical local path
- the lane is ready for git and OCI resolver batches without another manifest
  model rewrite

## Closeout

This batch landed:

- the first shared `ResolvedBundleSource` code shape
- one shared materialized-source resolver for shipped and path sources
- one shared defaults-loading seam on top of the materialized source result
- stable current behavior for shipped bundles and local path bundles

## Next Task

Execute
[`630-add-git-bundle-source-resolver-and-cache-identity.md`](./630-add-git-bundle-source-resolver-and-cache-identity.md).
