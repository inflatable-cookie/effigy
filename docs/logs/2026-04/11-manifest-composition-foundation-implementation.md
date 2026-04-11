# 2026-04-11 - Manifest Composition Foundation Implementation

Roadmap: `g02.002`

## Summary

Shipped the first implementation slice for manifest composition.

Effigy now supports `[manifest].include` for nested partial fragments, enforces
path-scoped override and conflict rules during load, and exposes a first native
inspection surface through `effigy config --inspect`.

The first proof boundary is also complete: a split `tasks + docs_policy`
fixture passes through the same composed-manifest path, which means the feature
is no longer task-only doctrine.

## Changes

- added composed-manifest loading and merge logic under
  `src/runner/manifest/composition.rs`
- routed normal manifest load, doctor, scan-backed loading, and catalog
  discovery through the composed loader
- added `effigy config --inspect` in text and JSON modes
- extended config schema/reference/help docs for `[manifest].include`
- proved the feature against a cross-feature `tasks + docs_policy` split

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: baseline `composition was a planned contract with no real runtime
  surface` -> current `composition loads real split manifests, fails on
  conflicts predictably, and exposes one native inspection surface`
- Remaining gap: `operator explainability and focused inspection still need one
  hardening batch before downstream planning should depend on this surface
  heavily`

## Validation Performed

- command: `cargo test`
  - result: passed
- command: `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
  - result: passed
- command: `effigy qa:docs`
  - result: passed

## Next Task

Execute the active ready card in
`docs/specs/batch-cards/006-harden-composition-explainability-and-operator-ux.md`,
then decide whether one final composition hardening/proof batch is needed
before `g02.003` starts depending on this surface.
