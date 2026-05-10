# 628 - Add Unified Bundle Base Model And `base_path` Removal

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Land the first code-bearing slice for `g04.022`: the manifest/config boundary
for unified bundle sources.

## Scope

- replace the legacy `base`/`base_path` split with one typed source model
- keep string `base = "underlay"` sugar working
- add explicit `base = { type = ... }` parsing for `shipped`, `path`, `git`,
  and `oci`
- reject `base_path` with the locked migration error
- update bundle schema/config rendering and parser tests

## Acceptance

- bundle config parsing accepts the locked string and block forms
- `base_path` now fails with the contract error instead of parsing
- the internal manifest model carries one typed source boundary
- schema/config docs reflect the new grammar
- parser and serde round-trip coverage exists for all accepted forms

## Next Task

Implement the typed bundle-source manifest model and `base_path` removal, then
advance the lane to the shared source-materialization batch.
