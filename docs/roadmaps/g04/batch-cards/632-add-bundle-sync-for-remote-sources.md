# 632 - Add Bundle Sync For Remote Sources

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Goal

Add an explicit refresh surface for git and OCI bundle sources without widening
into bundle inspect yet.

## Scope

- add `effigy bundle sync`
- resolve the current repo bundle source through the shared source seam
- refresh git bundle sources against the declared ref
- refresh OCI bundle sources against the declared reference
- report whether the local cache changed
- keep shipped and path bundle sources as no-op or not-applicable

## Acceptance

- `effigy bundle sync` works for git-backed bundle sources
- `effigy bundle sync` works for OCI-backed bundle sources
- sync can refresh a stale cached source into a current local bundle root
- shipped/path bundle sources stay bounded and clearly reported
- focused CLI and source-refresh proofs cover both remote source kinds

## Outcome

- `effigy bundle sync` now refreshes repo-local git and OCI bundle sources
- shipped/path bundle sources stay bounded as not-applicable
- sync reports whether the cached bundle root changed and returns
  `effigy.bundle.sync.v1` in JSON mode
- remote-source refresh proofs now cover both git and OCI source kinds

## Next Task

Execute [`633-extend-bundle-inspect-with-source-metadata.md`](./633-extend-bundle-inspect-with-source-metadata.md).
