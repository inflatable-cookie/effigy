# 1096 - Fix Catalog Fragment Listing

Roadmap: [`../041-catalog-fragment-listing-papercut.md`](../041-catalog-fragment-listing-papercut.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/043-feature-placement-and-surface-migration-contract.md`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)
Papercut: [`PAPERCUTS.md`](../../../../PAPERCUTS.md)

Status: Complete
Owner: `effigy-catalog` bundled fragment inventory
Created: 2026-09-01
Ready since: 2026-09-01 papercut triage on current `main`
Completed: 2026-09-01
Evidence: [`../../../logs/2026-09/01-133154-catalog-fragment-listing-1096.md`](../../../logs/2026-09/01-133154-catalog-fragment-listing-1096.md)

## Purpose

Stop root catalog assets from being advertised as callable service fragments.

## Observed Failure

`CatalogResolver::list_bundled_fragments` takes the first path component of
every embedded catalog asset. Root `README.md` and
`compose.override.example.yml` therefore appear in `effigy service list`, which
reports 16 fragments when the bundled catalog contains 14 service manifests.

## Work

- define bundled membership by the presence of `<name>/service.toml`
- keep the inventory sorted and deduplicated
- add non-vacuous tests for root files, a manifest-less directory, valid
  fragments, and command text/JSON output
- preserve filesystem overrides, installed packs, extraction, and service
  resolution
- close the selected papercut and write one compact evidence log

## Acceptance

- [x] root documentation and example files are not fragment names
- [x] a first-level directory without `service.toml` is not a fragment
- [x] every bundled `service.toml` parent is listed exactly once and sorted
- [x] current `service list` text and JSON expose the corrected inventory
- [x] no catalog layer, pack, schema, selection, extraction, or CLI contract
      changes
- [x] focused catalog/CLI tests, `effigy qa`, fmt, clippy, and diff checks pass
- [x] papercut, roadmap, card, evidence, and active next-task pointers close
      honestly and return to publication planning

## Review Oracle

Falsify these counterexamples before PR creation:

1. A root asset such as `README.md` still becomes a fragment name.
2. `compose.override.example.yml` still appears in text or JSON service output.
3. A first-level directory containing assets but no `service.toml` is listed.
4. Filtering non-fragments drops or duplicates a real service manifest parent,
   or changes deterministic order.
5. The repair changes project/user/pack layering, extraction, service lookup,
   command grammar, or output schema rather than only bundled inventory.

## Validation

- focused `effigy-catalog` unit/integration tests
- focused `service list` text and JSON command-output tests
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping every oracle row to exact proof, recording
the corrected current inventory, validation, papercut closure, and return to
official publication planning.

## Stop Conditions

Stop if the fix needs a manifest/schema change, a new fragment-kind rule,
catalog-pack behavior, concrete asset movement, a CLI/output contract change,
release/workflow authority, or an unrelated cleanup.

## Next Task

Return to planning for official catalog-pack publication and concrete-asset
cutover under contract `043`.
