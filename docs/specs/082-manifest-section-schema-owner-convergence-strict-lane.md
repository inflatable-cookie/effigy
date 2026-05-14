# 082 - Manifest Section Schema Owner Convergence Strict Lane

Roadmap: [`g05.017`](../roadmaps/g05/017-manifest-section-schema-owner-convergence.md)

Status: Active
Owner: Platform
Created: 2026-05-14

## Purpose

Converge duplicated `[manifest]` section owners so root manifests, included
fragments, and bundle defaults parse and validate through one canonical shape.

## Lane Posture

Posture: `strict-ready`

This lane is executable because the duplicated owner surfaces are identified,
the user-facing schema is already known, and the work can land in bounded
manifest-crate slices.

## Hard Boundaries

- no user-facing `[manifest]` schema redesign
- no include/extend precedence rewrite
- no bundle system redesign
- no runtime behavior widening beyond duplicate-owner bug fixes
- no release execution
- no `.github/workflows/` edits

## Execution Order

1. `737` open the manifest-section convergence lane and wire the ready chain
2. `738` extract one canonical manifest-section owner in `effigy-manifest`
3. `739` switch root composition and bundle defaults to the canonical owner and
   add regression proof
4. `740` close the lane and refresh currentness surfaces

## Ready Chain

- `737` is ready now
- `738` is ready after `737`
- `739` is ready after `738`
- `740` is ready after `739`

## Auto-Continuation Envelope

Auto-start is enabled while:

- each prior card closes green
- no new schema judgment is required
- no call site needs fields that the canonical owner should intentionally reject

Stop and replan if implementation discovers root and bundle `[manifest]`
surfaces are not actually meant to share the same field set.

## Acceptance

This lane is complete when:

- one canonical `[manifest]` section owner exists
- root composition and bundle defaults both reuse it
- regression tests cover root, included fragment, and bundle-defaults paths
- currentness surfaces point at the next queued schema-shape lane or no lane

## Next Task

Execute `737` now, then continue directly into `738`.
