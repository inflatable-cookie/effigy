# 082 - Manifest Section Schema Owner Convergence Strict Lane

Roadmap: [`g05.017`](../roadmaps/g05/017-manifest-section-schema-owner-convergence.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Converge duplicated `[manifest]` section owners so root manifests, included
fragments, and bundle defaults parse and validate through one canonical shape.

## Lane Posture

Posture: `strict-closed`

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
2. `738` complete: canonical manifest-section owner extracted in `effigy-manifest`
3. `739` complete: root composition and bundle defaults now reuse the canonical owner with regression proof
4. `740` complete: lane closed and currentness surfaces refreshed

## Ready Chain

- `737` is complete
- `738` is complete
- `739` is complete
- `740` is complete

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

No next task. Lane `082` is closed.
