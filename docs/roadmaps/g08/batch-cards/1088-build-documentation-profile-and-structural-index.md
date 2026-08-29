# 1088 - Build Documentation Profile And Structural Index

Roadmap: [`../035-repository-defined-documentation-graph.md`](../035-repository-defined-documentation-graph.md)
Architecture: [`../../../architecture/024-repository-defined-documentation-graph.md`](../../../architecture/024-repository-defined-documentation-graph.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)
Spec: [`../../../specs/108-documentation-graph-profiles-strict-lane.md`](../../../specs/108-documentation-graph-profiles-strict-lane.md)

Status: Complete
Owner: manifest and codegraph documentation semantics
Created: 2026-08-29
Ready after: operator selected the repository-defined documentation graph lane

## Purpose

Build the repository-neutral semantic foundation before exposing a public query.
The batch owns typed profile configuration, deterministic validation, exact
Markdown sections, fields/relations, and profile-aware freshness.

## Owner And Seam

`effigy-manifest` owns typed `[docs_policy.graph]` config. `effigy-codegraph`
owns compiled matching and structural extraction. Reuse the existing Markdown
parser, graph primitives, storage, and refresh lifecycle. Do not add CLI grammar
or Northstar-specific defaults in this card.

## Work

- add typed graph roots, field, currentness, kind, and relation definitions
  under `ManifestDocsPolicyConfig`
- validate tokens, labels, value sets, authority bounds, root containment,
  selectors, currentness references, and overlapping kind matches
- compile one deterministic profile for the selected repository
- make the normalized profile fingerprint part of documentation freshness
- replace whole-file Markdown heading spans with exact hierarchical section
  spans while preserving existing document/link/code-fence behavior
- extract configured `Label: value` facts outside code fences
- type configured links by metadata label or containing heading
- preserve baseline document/section/link extraction with no profile
- emit exact provenance and actionable diagnostics for invalid or ambiguous
  semantic input
- add focused manifest and codegraph fixtures using vocabulary unrelated to
  Northstar

## Acceptance

- [x] missing profile selects baseline mode without error
- [x] arbitrary field, kind, and relation tokens parse and round-trip through
      the composed manifest
- [x] invalid roots, escapes, overlaps, duplicate single-valued fields, and
      bad currentness references fail deterministically
- [x] section spans end at the correct peer/ancestor heading or EOF
- [x] field lines inside code fences are ignored
- [x] configured label and heading links produce only the requested typed edges
- [x] a profile-only edit forces semantic re-indexing
- [x] no Northstar path, status, or kind appears in generic runtime logic
- [x] no public command or JSON contract is added in this card

## Validation

- focused `cargo test -p effigy-manifest` profile parsing and validation tests
- focused `cargo test -p effigy-codegraph` Markdown/profile/freshness tests
- `cargo fmt --all -- --check`
- `cargo clippy -p effigy-manifest -p effigy-codegraph --all-targets -- -D warnings`
- changed-file affected analysis through `effigy graph`
- `git diff --check`

## Evidence Requirement

Close with one dated log containing fixture cases, test counts, profile
fingerprint proof, exact-span examples, affected analysis, and the explicit
readiness transition for card `1089`. Evidence:
[`30-004016-documentation-graph-1088.md`](../../../logs/2026-08/30-004016-documentation-graph-1088.md).

## Stop Conditions

Stop if the profile requires a second parser or graph store, if manifest
composition cannot identify one repository authority, if exact sections need
model inference, or if generic extraction needs Northstar-specific branches.

## Next Task

Execute ready card
[`1089`](./1089-add-bounded-documentation-context-query.md).
