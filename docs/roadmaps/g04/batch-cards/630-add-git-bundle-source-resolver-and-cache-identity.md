# 630 - Add Git Bundle Source Resolver And Cache Identity

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Land the first remote bundle-source resolver: git.

## Scope

- add the first git bundle-source resolver behind the shared source seam
- normalize git URL cache identity
- materialize git sources into the locked cache path
- support explicit refs with the current default-to-`main` behavior
- surface bounded clone/ref/auth/network failures
- keep stale/update detection minimal and local to the git source path

## Acceptance

- git bundle sources resolve through the shared `ResolvedBundleSource` seam
- cache paths are stable across equivalent git URL forms
- branch, tag, and sha refs load through one bounded resolver
- current shipped/path bundle behavior stays unchanged
- focused git resolver tests cover happy path and direct failure modes

## Next Task

Implement the git bundle-source resolver and cache identity rules, then
advance the lane to the OCI source batch.
