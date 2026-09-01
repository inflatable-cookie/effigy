# g08.041 Catalog Fragment Listing Papercut

Status: Complete
Created: 2026-09-01
Completed: 2026-09-01
Card: [`1096`](./batch-cards/1096-fix-catalog-fragment-listing.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)
Papercut: [`PAPERCUTS.md`](../../../PAPERCUTS.md)
Evidence: [`../../logs/2026-09/01-133154-catalog-fragment-listing-1096.md`](../../logs/2026-09/01-133154-catalog-fragment-listing-1096.md)

## Purpose

Make `effigy service list` report only bundled catalog fragments that carry a
`service.toml`, matching the filesystem-layer definition of a fragment.

## Decision

- A bundled fragment is a first-level catalog directory containing
  `<name>/service.toml`.
- Root documentation, examples, and directories without a service manifest are
  not fragments.
- Preserve sorting, deduplication, override layering, pack selection, service
  resolution, extraction, and output schemas.

## Scope

- `CatalogResolver::list_bundled_fragments`
- focused catalog and command-output recurrence tests
- papercut and evidence closeout

## Boundary

- no catalog-pack acquisition, publication, update, retention, or asset moves
- no service manifest/schema change
- no CLI grammar, text/JSON schema, or override-layer change
- no release, workflow, S3, provider, or unrelated papercut work

## Cards

- [x] [`1096`](./batch-cards/1096-fix-catalog-fragment-listing.md) — complete

## Acceptance

- root-level `README.md` and `compose.override.example.yml` are absent from the
  bundled fragment inventory
- a directory without `service.toml` is absent
- every directory with `service.toml` remains present exactly once in sorted
  order
- `effigy service list` text and JSON derive from the corrected inventory
- focused and full repository validation pass

## Next Task

Return to planning for official catalog-pack publication and concrete-asset
cutover under contract `043`.
