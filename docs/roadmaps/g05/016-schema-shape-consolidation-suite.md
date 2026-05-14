# g05.016 - Schema Shape Consolidation Suite

Status: Complete
Depends on: `g05.015`

## Goal

Reduce schema drift by converging duplicated manifest and task-like TOML shape
owners onto shared canonical definitions without changing the supported schema
surface.

## Evidence

- the recent bundle bug showed that root manifest parsing accepted
  `[manifest].minimum_effigy_version` while bundle defaults rejected it because
  they used a narrower duplicate serde shape
- `crates/effigy-manifest/src/composition.rs` and
  `crates/effigy-manifest/src/bundles.rs` still define separate `[manifest]`
  section structs
- task-like definitions are still wrapped through multiple local unions across
  tasks, bootstrap, state hooks/capture tasks, and inline task support

## Scope

- map the duplicated schema-owner surfaces for `[manifest]` and task-like
  definitions
- converge reusable shape owners into `effigy-manifest`
- keep syntax compatibility for the currently supported TOML shapes
- add proof that root manifests, bundle defaults, bootstrap, and state surfaces
  all resolve through the same intended building blocks where they should

## Ordered Follow-Up Lanes

1. [`017-manifest-section-schema-owner-convergence.md`](./017-manifest-section-schema-owner-convergence.md)
2. [`018-task-like-definition-schema-convergence.md`](./018-task-like-definition-schema-convergence.md)
3. [`019-schema-shape-regression-proof-and-closeout.md`](./019-schema-shape-regression-proof-and-closeout.md)

## Non-Goals

- no release execution
- no user-facing schema redesign
- no breaking TOML grammar cleanup hidden inside refactoring
- no broad manifest crate rewrite unrelated to duplicated shape ownership

## Acceptance Criteria

- duplicated schema owners are reduced for the targeted surfaces
- canonical shape owners live in `effigy-manifest`
- the supported syntax remains stable unless an explicit later roadmap changes it
- focused tests prove shared behavior across the previously split paths

## Completed

- `g05.017` completed canonical `[manifest]` section ownership.
- `g05.018` completed canonical task-like definition ownership for tasks,
  bootstrap, and state reference-or-inline surfaces.
- `g05.019` completed focused regression proof and closeout notes.

## Next Task

Schema-shape consolidation is complete. Move to the queued reusable-core
hardening suite when ready.
