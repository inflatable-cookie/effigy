# 628 - Add Unified Bundle Base Model And `base_path` Removal

Lane: [`065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md`](../065-remote-bundle-sources-git-and-oci-delivery-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

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

## Closeout

This batch landed:

- one typed bundle-source manifest model
- string shipped sugar plus legacy shipped `name` alias
- typed `path`, `git`, and `oci` block parsing
- hard `base_path` rejection with migration guidance
- direct config/export/help surface updates for the new path form

## Next Task

Execute
[`629-add-shared-bundle-source-resolver-and-path-source-materialization.md`](./629-add-shared-bundle-source-resolver-and-path-source-materialization.md).
