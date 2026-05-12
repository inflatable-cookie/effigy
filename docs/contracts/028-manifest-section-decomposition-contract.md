# Manifest Section Decomposition Contract

Generation: `g04`
Roadmap: [`../roadmaps/g04/036-manifest-section-decomposition.md`](../roadmaps/g04/036-manifest-section-decomposition.md)
Strict lane: [`../specs/072-manifest-section-decomposition-strict-lane.md`](../specs/072-manifest-section-decomposition-strict-lane.md)
Status: Accepted
Owner: Platform
Updated: 2026-05-12

## Purpose

Define the structural boundary for decomposing Effigy's manifest parsing files.

The manifest crate is the config authority for many product surfaces. Its
largest files should be section-owned rather than acting as long-lived
catch-all modules.

## Hard Boundaries

- no TOML grammar changes
- no composed manifest behavior changes
- no public command behavior changes
- no bundle source behavior changes
- no deploy provider package behavior changes
- no state stack behavior changes
- no app-specific parsing logic
- no `.github/workflows/` edits
- no release execution

## Decomposition Rules

- preserve public Rust API paths unless a card explicitly scopes a re-exported
  move
- preserve error text unless changing it is explicitly accepted
- split by durable section ownership, not by arbitrary line counts
- keep tests near the section owner where practical
- do not move command logic into `effigy-manifest`
- do not create speculative generic parser frameworks

## Current Ownership Map

`669` confirmed these concrete owner groups:

- `bundles.rs`: public bundle schema, bundle source selection, git/OCI/path
  materialization, shared cache identity, sync/inspect reports, local bundle
  descriptor parsing, input validation, template rendering, and bundle input
  merge helpers.
- `config_sections.rs`: bundle base/config grammar, task defaults, isolation,
  shell, package manager, scan sections, env schema, containers/systems/
  workspaces/services/DNS/exec aliases/lifecycle/host/data, docs/demo policy,
  bootstrap, distribution, and release config.
- `composition.rs`: manifest import/root semantics, local overlay discovery,
  source-map recording, conflict handling, and bundle default composition.
- `lib.rs`: public facade and `TaskManifest` aggregate.

The first implementation split should target bundle source materialization and
cache behavior. That cluster is cohesive, already tested by git/OCI bundle
source tests, and can move behind an internal module without changing TOML
grammar or public command behavior.

## Test Coverage Map

Existing coverage for the first split:

- inline tests in `bundles.rs` cover canonical git cache identity, git cache
  materialization, OCI cache materialization, stale digest detection, pull
  failure reporting, source sync changes, and cache refresh behavior.
- `crates/effigy-manifest/tests/local_bundle.rs` covers path bundle defaults,
  exported Underlay/Decodelabs bundle fixtures, input validation, and removed
  `base_path` errors.
- `crates/effigy-manifest/tests/decodelabs_bundle.rs`,
  `underlay_bundle.rs`, and `underlay_starter.rs` cover exported bundle
  fixture compatibility.
- `composition.rs` inline tests cover manifest import/root/extend behavior and
  should remain unchanged by the bundle source split.

Coverage gaps for later cards:

- `config_sections.rs` has broad downstream coverage through composition and
  runner tests, but fewer section-local parse tests.
- deploy/state/object-store config are parsed outside the manifest crate in
  current command surfaces, so any later move must preserve raw manifest value
  ownership until a specific contract changes it.

## Acceptance Boundary

This contract is satisfied when:

- `bundles.rs` and `config_sections.rs` are materially smaller or become facade
  modules
- section-specific tests cover moved parsing behavior
- downstream callers remain compatible
- representative manifest composition tests pass
- god-file scan pressure is reduced without hiding ownership in vague utility
  modules

## Accepted Shape

The accepted v0.6.x cleanup shape is:

- `bundles.rs` remains the bundle descriptor/default/template owner and facade.
- `bundles/source.rs` owns bundle source selection, materialization, cache
  identity, sync, inspect, and source tests.
- `config_sections.rs` remains the public facade and compatibility test owner.
- section-owned config modules sit under `config_sections/` for bundle, common
  config, container/data, demo/docs policy, bootstrap, distribution, and release.

The lane deliberately did not move deploy/state parsing into
`effigy-manifest`. Those surfaces currently use raw manifest values so command
domain crates can evolve without forcing all manifest consumers to depend on
deployment or state semantics.
