# 1072 - Add Explicit Member And Typed Mount Schema

Roadmap: [`../028-explicit-catalog-membership.md`](../028-explicit-catalog-membership.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/037-explicit-catalog-membership-contract.md`](../../../contracts/037-explicit-catalog-membership-contract.md)
Spec: [`../../../specs/archive/101-explicit-catalog-membership-strict-lane.md`](../../../specs/archive/101-explicit-catalog-membership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-08-10
Ready after: contract `037` promotion and operator approval of spec `101`

## Purpose

Add the typed manifest foundation required for explicit membership without yet
changing which catalogs runtime routing selects.

## Owner And Seam

`effigy-manifest` owns `[catalog.members]` and the untagged string/table system
mount model. `effigy-containers` consumes typed mount accessors for rendering.
`effigy-routing` may consume the typed source accessor during this schema batch
but does not switch membership policy until card `1073`.

## Work

- add `catalog.members` as a handle-to-directory map
- replace raw system/workspace mount strings with one untagged typed mount enum
- model structured `member` versus `source`, optional `target`, string-array
  `options`, and source-only `catalog`
- centralize exclusive-field and non-empty-value validation
- expose typed source/target/options helpers needed by routing and containers
- preserve current legacy string parsing, basename target, options, and
  isolation-adoption behavior
- update doctor schema recognition for the new grammar without removing
  discovery-era keys yet
- add focused valid/rejection/composition/rendering fixtures

## Acceptance

- [x] named member maps parse through root manifest composition
- [x] valid member-reference, inline-catalog, ordinary structured, and legacy
      mounts parse into one typed model
- [x] neither/both source forms, member-plus-catalog, invalid options, and empty
      values fail precisely
- [x] legacy strings render byte-for-byte as before
- [x] omitted structured targets use the current basename-derived target
- [x] producer isolation auto-adoption still resolves typed mount sources
- [x] catalog routing behavior remains unchanged in this foundation card

## Validation

- focused `effigy-manifest` catalog/member and system-mount tests
- focused `effigy-containers` workspace mount and isolation tests
- focused `effigy-routing` mount-source compatibility tests
- doctor manifest-schema tests
- `cargo fmt --all -- --check`
- focused Clippy on touched crates
- affected-test selection from the graph after implementation

## Evidence

See
[`2026-08/10-095639-explicit-catalog-schema-foundation.md`](../../../logs/archive/2026-08/10-095639-explicit-catalog-schema-foundation.md).

## Stop Conditions

Stop if the grammar needs globs, absolute named members, recursive child
expansion, duplicate mount parsers, or a container-owned membership decision.

## Next Task

Execute ready card
[`1073`](./1073-cut-routing-over-to-explicit-membership.md).
