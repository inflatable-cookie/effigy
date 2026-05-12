# 669 - Map Manifest Section Ownership And Test Coverage

Roadmap: [`../036-manifest-section-decomposition.md`](../036-manifest-section-decomposition.md)
Strict lane: [`../../../specs/072-manifest-section-decomposition-strict-lane.md`](../../../specs/072-manifest-section-decomposition-strict-lane.md)
Contract: [`../../../contracts/028-manifest-section-decomposition-contract.md`](../../../contracts/028-manifest-section-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Map symbols and tests in `bundles.rs` and `config_sections.rs` before moving
manifest code.

## Scope

- classify public structs, enums, functions, and helper clusters in
  `bundles.rs`
- classify public structs, enums, functions, and helper clusters in
  `config_sections.rs`
- identify which tests currently cover each section
- choose the safest first implementation slice for `670`
- update contract `028` if the discovered ownership differs from the candidate
  map

## Non-Goals

- no code movement in this card
- no manifest grammar changes
- no public API changes
- no command behavior changes

## Suggested Evidence Commands

```sh
rg -n "^(pub |pub\\(|fn |struct |enum |impl )" crates/effigy-manifest/src/bundles.rs crates/effigy-manifest/src/config_sections.rs
rg -n "bundle|deploy|state|object|container|manifest|root|import" crates/effigy-manifest/tests src tests docs/contracts docs/guides
wc -l crates/effigy-manifest/src/bundles.rs crates/effigy-manifest/src/config_sections.rs
```

## Acceptance

- `bundles.rs` ownership clusters are listed
- `config_sections.rs` ownership clusters are listed
- relevant test coverage is listed
- first implementation slice is selected
- `670` can execute without a fresh planning pass

## Outcome

- confirmed `bundles.rs` owns bundle schema, source selection, git/OCI/path
  materialization, cache identity, local descriptor parsing, input validation,
  template rendering, and input merge helpers
- confirmed `config_sections.rs` owns unrelated section grammar for bundle,
  tasks, isolation, package managers, scans, env schema, containers, systems,
  workspaces, docs/demo, bootstrap, distribution, and release
- recorded the ownership and test coverage map in contract `028`
- selected bundle source materialization and cache behavior as the first safe
  split for `670`

## Validation

- docs review
- `git diff --check`

## Next Task

Execute `670` with the selected first split.
