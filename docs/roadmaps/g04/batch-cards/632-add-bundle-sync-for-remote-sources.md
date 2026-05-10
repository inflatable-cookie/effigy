# 632 - Add Bundle Sync For Remote Sources

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Ready
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

## Next Task

Add the `bundle sync` parser, dispatch, and remote-source refresh behavior on
top of the completed shared bundle-source resolver seam.
