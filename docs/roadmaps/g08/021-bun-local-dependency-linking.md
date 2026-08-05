# g08.021 - Bun Local Dependency Linking

Status: Complete
Depends on: `g08.020`

## Goal

Implement reversible, save-less Bun package linking for the complete matching
library closure with zero manifest and lockfile churn.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Target envelope: local Bun packages resolve through verified consumer
  symlinks while committed package and lock state remains byte-for-byte stable.
- Vision target delta: ephemeral Bun link behavior gains desired state, closure
  enforcement, drift repair, and peer diagnostics.

## Scope

- inventory root and workspace library packages
- match direct/transitive packages in the consumer tree
- register every matched package with save-less `bun link`
- link every matched package into the consumer without `--save`
- preserve package.json and Bun lockfiles byte-for-byte
- record registration ownership without clobbering foreign global links
- reference-count Effigy-owned registrations through the machine-local index
- verify consumer symlinks resolve to canonical library package paths
- make re-link repair symlinks lost after `bun install`
- unlink only the selected consumer/library closure
- unregister only when Effigy can prove the registration is no longer shared
- detect partial local/registry closure and duplicate framework peer resolution
- report per-package before/after source and drift evidence

## Non-Goals

- no package publication
- no package renaming
- no `package.json` overrides/resolutions
- no Bun `--save`
- no bundler-specific config mutation to force dedupe

## Execution Plan

- [x] [`1057`](./batch-cards/1057-plan-bun-full-closure-and-registration-ownership.md)
      — plan the full Bun closure, save-less process intents, immutable
      manifest/lock guards, and safe registration ownership without mutation
- [x] [`1058`](./batch-cards/1058-apply-and-verify-bun-links.md)
      — apply explicit `--no-save` registrations and consumer links, prove
      immutable manifests/locks, verify the full closure, and expose link CLI
- [x] [`1059`](./batch-cards/1059-apply-bun-unlink-peer-diagnostics-and-closeout.md)
      — unlink exact consumer closures, release only provably unshared owned
      registrations, diagnose duplicate peers, and close the Bun milestone

## Acceptance Criteria

- [x] root-only and multi-package library fixtures are inventoried
- [x] the complete matching closure is linked or no changes are applied
- [x] package manifests and lockfiles remain byte-identical
- [x] conflicting global registration paths are refused
- [x] healthy pre-existing registrations are not claimed or removed
- [x] re-link repairs consumer symlink drift idempotently
- [x] unlink preserves registrations still referenced by other desired links
- [x] concurrent consumers cannot lose or corrupt registration ownership state
- [x] peer duplication reports package paths and actionable dedupe guidance
- [x] dry-run reports registrations and symlink deltas without mutation

## Validation

- focused Bun inventory/state/registration fixture tests
- functional save-less `bun link` proof against the supported Bun version
- install-removes-link drift and re-link repair proof
- peer duplication fixture
- `effigy qa:ci:fast`

## Next Task

Execute ready observed-health card `1060` under `g08.022`.
