# 629 - Add Shared Bundle Source Resolver And Path Source Materialization

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

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

## Next Task

Implement the shared shipped/path resolver seam, then advance to the first
git-source batch.
