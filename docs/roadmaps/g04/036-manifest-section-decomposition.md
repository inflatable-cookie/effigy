# 036 - Manifest Section Decomposition

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-12
Depends on:
- [`035-state-domain-extraction.md`](./035-state-domain-extraction.md)

## Goal

Split oversized manifest parsing files into section-owned modules while keeping
the composed manifest format and public behavior stable.

## Evidence

- `crates/effigy-manifest/src/bundles.rs` is 2,060 lines
- `crates/effigy-manifest/src/config_sections.rs` is 1,944 lines
- both files were flagged by `effigy scan god-files --json`
- remote bundles, state, deploy, provider packages, containers, and object-store
  configuration are all increasing manifest pressure

## Scope

- inventory manifest sections and parsing ownership
- split bundle source parsing from bundle source materialization where useful
- split config section parsing by domain section
- keep public Rust APIs stable unless a clearer internal module path is enough
- preserve TOML shapes and error messages unless a change is explicitly scoped
- add focused tests around import/root/config composition boundaries

## Non-Goals

- no manifest format redesign
- no new config sections
- no bundle source behavior changes
- no provider package behavior changes
- no app-specific config logic

## Candidate Module Shape

Possible internal modules:

- `bundle_config`
- `bundle_source`
- `deploy_config`
- `state_config`
- `container_config`
- `object_store_config`
- `root_config`
- `imports`
- `section_errors`

The exact split should follow actual dependency seams rather than file-size
targets alone.

## Acceptance Criteria

- `bundles.rs` and `config_sections.rs` are reduced to bounded module owners or
  facade files
- section-specific parsing tests live beside the owning modules
- composed manifest behavior is unchanged
- import/root semantics remain covered
- no downstream command code needs to know the internal module split

## Outcome

- split bundle source/cache behavior into `bundles/source.rs`
- split config-section schema into section-owned modules under
  `config_sections/`
- kept public manifest imports stable through facade re-exports
- reduced `bundles.rs` from 2,060 lines to 856 lines
- reduced `config_sections.rs` from 1,944 lines to 745 lines
- removed manifest files from god-file scan findings

## Suggested Batch Cards

- `668-open-manifest-section-decomposition-lane.md`
- `669-map-manifest-section-ownership-and-test-coverage.md`
- `670-split-bundle-source-and-cache-modules.md`
- `671-split-container-config-section.md`
- `672-split-remaining-config-sections.md`
- `673-close-manifest-section-decomposition-proof.md`

## Validation

- `effigy-manifest` tests
- manifest composition fixture tests
- `effigy config --json` against representative fixtures
- `effigy bundle inspect --json`
- `effigy deploy plan <fixture> --json`
- `effigy scan god-files --json`
- `git diff --check`

## Next Task

Execute `037` for deploy domain boundary hardening.
