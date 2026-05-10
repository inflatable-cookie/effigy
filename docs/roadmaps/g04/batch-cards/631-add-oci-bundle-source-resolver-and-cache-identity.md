# 631 - Add OCI Bundle Source Resolver And Cache Identity

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Land the OCI bundle-source resolver on top of the shared source seam and the
existing artifact substrate.

## Scope

- add the first OCI bundle-source resolver behind the shared source seam
- reuse the existing artifact/OCI auth and fetch behavior
- materialize OCI sources into the locked cache path
- capture digest-backed version hints
- surface bounded pull/auth/network/reference failures

## Acceptance

- OCI bundle sources resolve through the shared `ResolvedBundleSource` seam
- cache paths are stable for tag and digest forms
- pulled bundles materialize into a local bundle root consumable by the
  existing defaults loader
- current shipped/path/git bundle behavior stays unchanged
- focused OCI resolver tests cover happy path and direct failure modes

## Next Task

Implement the OCI bundle-source resolver and cache identity rules, then
advance the lane to the bundle sync and inspect surface batches.
