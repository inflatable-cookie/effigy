# Catalog Fragment Listing 1096 Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.041
Batch: 1096-fix-catalog-fragment-listing
Handoff: `20260901-131954-catalog-fragment-listing-1096.md`
Papercut: `service list` reports non-fragment bundled files as fragments

## Summary

- `CatalogResolver::list_bundled_fragments` treated every first path component
  of an embedded catalog asset as a fragment name, so root `README.md` and
  `compose.override.example.yml` appeared in `effigy service list` (16 names
  for 14 real services).
- Membership now requires a first-level `<name>/service.toml`, matching the
  card rule. Sorting and deduplication stay with the existing `list()` path.
- No pack, override-layer, schema, extraction, selection, CLI grammar, or
  output-schema changes.

## Corrected inventory

Exact bundled names, sorted once:

`dbgate`, `elasticsearch`, `mailpit`, `mariadb`, `memcached`, `minio`, `nginx`,
`node`, `pgweb`, `php-fpm`, `phpmyadmin`, `postgres`, `redis`,
`workspace-rust-bun`

## Review oracle → proof

1. Root asset such as `README.md` still becomes a fragment name —
   falsified by `fragment::tests::root_assets_are_not_fragment_names`, the
   exact integration inventory assert, and both CLI text/JSON tests rejecting
   `README.md`.
2. `compose.override.example.yml` still appears in text or JSON —
   falsified by the same unit, integration, and CLI proofs.
3. A first-level directory with assets but no `service.toml` is listed —
   falsified by
   `fragment::tests::manifest_less_directory_assets_are_not_fragments` and
   `inventory_from_mixed_paths_is_sorted_and_deduplicated`.
4. Filtering drops, duplicates, or reorders a real service parent —
   falsified by the exact sorted 14-name integration assert and the unit
   inventory helper proof.
5. The repair changes layering, extraction, lookup, grammar, or schema —
   falsified by diff scope (`list_bundled_fragments` + tests/docs only) and
   full `effigy qa` including pack and existing catalog integration coverage.

## Changes

- `crates/effigy-catalog/src/fragment.rs`: filter bundled inventory through
  `bundled_fragment_name_from_asset_path`; add focused unit tests.
- `crates/effigy-catalog/tests/integration/fragments.rs`: assert exact sorted
  inventory and absence of root assets.
- `tests/service_list_cli_tests.rs`: text and JSON command-output proofs.
- Closed papercut, roadmap `g08.041`, card `1096`, and Next Task pointers.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: bundled inventory advertised root docs/examples as fragments →
  inventory is exactly the `service.toml` parents
- Remaining gap: None for this papercut; official catalog-pack publication
  planning under contract `043` remains the Next Task

## Validation Performed

- `cargo test -p effigy-catalog --lib fragment::tests` — 5 passed
- `cargo test -p effigy-catalog --test integration list_bundled_fragments` —
  1 passed
- `cargo test --test service_list_cli_tests` — 2 passed
- `effigy qa` — 3632 passed (1 leaky), 1 skipped; docs and JSON-contract
  checks passed
- `cargo fmt --all -- --check` — passed
- `cargo clippy --all-targets -- -D warnings` — passed (existing
  `proc-macro-error2` future-incompat notice only)
- `git diff --check` — passed

## Risks

- Filesystem override and pack listing still enumerate directories without
  requiring `service.toml` at list time; that boundary was deliberately
  preserved. A later lane that wants one membership rule across all layers
  needs its own card.

## Next Task

- Return to planning for official catalog-pack publication and concrete-asset
  cutover under contract `043`. That lane needs a real OCI coordinate and
  explicit workflow-edit authority; it is not ready.
