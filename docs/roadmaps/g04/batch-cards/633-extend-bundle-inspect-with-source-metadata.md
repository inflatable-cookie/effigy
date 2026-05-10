# 633 - Extend Bundle Inspect With Source Metadata

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Widen `effigy bundle inspect` so it reports bundle source metadata for the
current repo, not just the shipped bundle input schema.

## Scope

- keep shipped bundle catalog inspection intact
- add repo-local source metadata when inspecting the active bundle source
- report source type, local cache path, version hint, and stale flag
- distinguish shipped/path/git/oci bundle sources clearly in text and JSON

## Acceptance

- `effigy bundle inspect` still works for shipped named bundles
- repo-local bundle inspection shows source metadata for local/remote sources
- JSON includes source metadata with stable field names
- focused inspect proofs cover shipped and remote-source bundle cases

## Next Task

Add source-aware inspect reporting on top of the completed shared resolver and
`bundle sync` surfaces.
