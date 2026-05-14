# 738 - Extract Canonical Manifest Section Owner

Roadmap: [`../017-manifest-section-schema-owner-convergence.md`](../017-manifest-section-schema-owner-convergence.md)
Strict lane: [`../../../specs/082-manifest-section-schema-owner-convergence-strict-lane.md`](../../../specs/082-manifest-section-schema-owner-convergence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Move the shared `[manifest]` serde and validation shape into one canonical
owner inside `effigy-manifest`.

## Scope

- extract the canonical `[manifest]` section struct into a reusable manifest-crate owner
- keep the currently supported field set stable: `include`, `extend`,
  `minimum_effigy_version`, and `root`
- keep validation behavior stable while moving it to the canonical owner seam

## Acceptance

- one canonical `[manifest]` owner exists in `effigy-manifest`
- root composition no longer owns a private duplicate shape
- focused manifest tests still pass

## Completed

- Added `crates/effigy-manifest/src/manifest_section.rs` as the canonical
  `[manifest]` section owner.
- Moved shared `[manifest]` serde fields and minimum-version validation into the
  canonical owner.
- Rewired root composition to use the shared owner.

## Validation

- `cargo test -p effigy-manifest minimum_effigy_version`
- `cargo fmt --all -- --check`
- `git diff --check`

## Stop Conditions

- stop if the bundle path needs a deliberately smaller field set than the root
  path instead of the same canonical shape

## Next Task

Execute `740` after the caller-adoption and regression-proof slice.
