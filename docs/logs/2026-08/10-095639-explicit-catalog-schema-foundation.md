# Explicit Catalog Schema Foundation

Status: complete
Created: 2026-08-10
Roadmap: g08.028
Batch: card-1072-explicit-catalog-schema

## Summary

- Added composed `[catalog.members]` handle-to-directory declarations.
- Replaced raw system/workspace mount strings with one typed string-or-table
  model.
- Added `member` and `source` structured forms, including source-only
  `catalog`, explicit targets, and string-array options.
- Preserved legacy rendering, basename-derived targets, discovery-era legacy
  mount membership, and isolation auto-adoption.
- Added doctor recognition and focused schema, composition, rendering,
  routing-compatibility, and end-to-end container-policy tests.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `MAINT`, `OPERATE`
- Movement: baseline `catalog identity and mount intent encoded in raw strings`
  -> current `explicit member declarations and typed mount intent available to
  routing and container consumers`
- Remaining gap: explicit declarations do not become the sole routing source
  until card `1073`.

## Validation Performed

- `cargo test -p effigy-manifest`
  - result: pass, 112 unit tests plus integration suites
- focused `effigy-containers`, `effigy-routing`, and `effigy-doctor` tests
  - result: pass, including named-member Compose rendering and isolation
    adoption parity
- `cargo clippy -p effigy-manifest -p effigy-containers -p effigy-routing -p effigy-doctor --all-targets -- -D warnings`
  - result: pass
- `effigy graph affected --stdin --json`
  - result: current graph; broad shared-manifest impact selected full regression
- `effigy test`
  - result: 1,634 tests passed; three pre-existing gateway tests blocked for
    more than three minutes while probing the live container runtime and were
    interrupted. The three exact tests then passed individually with container
    binaries absent, exercising their intended unavailable-runtime path.
- `effigy qa:docs`
  - result: pass
- `git diff --check`
  - result: pass

## Boundaries

Routing still uses discovery-era membership. No descendant-walk deletion,
selector-policy cutover, workflow edit, or release mutation occurred.

## Next Task

Execute ready card
[`1073`](../../roadmaps/g08/batch-cards/1073-cut-routing-over-to-explicit-membership.md).
