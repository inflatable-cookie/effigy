# Shared Dispatcher and Exec Collapse Contract

Generation: `g04`
Roadmap: [`../roadmaps/g04/026-shared-dispatcher-and-exec-collapse.md`](../roadmaps/g04/026-shared-dispatcher-and-exec-collapse.md)
Strict lane: [`../specs/069-shared-dispatcher-and-exec-collapse-strict-lane.md`](../specs/069-shared-dispatcher-and-exec-collapse-strict-lane.md)
Status: Draft
Owner: Platform
Updated: 2026-05-10

## Purpose

Lock the structural-only boundary for the next runtime simplification pass:

- shared JSON/text result rendering
- collapsed routed container-exec variants
- shared release-stage control flow

This lane is about duplication removal, not surface changes.

## Hard Boundaries

- no CLI grammar changes
- no JSON schema id/version changes
- no changes to command success/error meaning
- no `.github/workflows/` edits
- no release execution

## Dispatcher Boundary

The shared dispatcher is only for command surfaces that already carry both:

- a JSON value
- a text rendering

It must not invent new report shapes or coerce command-specific error
classifications into a fake common type.

The first helper owns only:

- render success result from existing json/text payloads
- render existing command failure payloads without changing their schema or text

It does not own:

- prompting
- side effects
- command-local planning
- exit-code policy

## Exec Collapse Boundary

The routed container-exec collapse is limited to the current near-duplicate
variants:

- run vs capture
- explicit policy vs resolved policy

The collapse must preserve:

- current routing behavior
- current policy override behavior
- current capture vs inherit behavior
- current error text

Public callers may keep the same function names if thin wrappers are the safest
way to land the shared internal seam.

## Release Stage Boundary

The release-stage helper may unify only the shared control-flow shape behind:

- `prepare`
- `execute`

It must not blur the stage-specific mutation boundary. `prepare` and `execute`
still own different side effects and different proof surfaces.

## Initial Execution Order

1. land the shared result-render helper
2. migrate a few low-risk command owners first
3. collapse routed container-exec duplication
4. extract release-stage shared control flow

## Acceptance Boundary

This lane is ready to close when:

- at least the selected command owners share one result-render seam
- routed container-exec duplication is collapsed behind one internal path
- release prepare/execute share one bounded stage helper
- no user-facing behavior drift is introduced
