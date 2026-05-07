# 511 - Select Artifact Staging Migration or Foundation Closeout

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Decide whether the next `g04.005` slice should move artifact staging internals
or close the first foundation pass.

## Scope

- inspect remaining DB seed and dump ownership in runner modules
- decide whether artifact staging request construction can move into
  `effigy-data` without adding side-effect dependencies
- decide whether dump/seed target resolution should remain runner-local until a
  manifest adapter card
- create the next implementation card or a closeout card

## Non-Goals

- no code changes unless they are small documentation/front-door corrections
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next bounded `g04.005` card is ready.

## Decision

Continue with one more foundation slice. Artifact transport must stay in the
runner, but `effigy-data` can still own pure seed staging intent: local source
path resolution, artifact root selection, and OCI pull destination planning.

Manifest-backed target resolution should remain runner-local until a later
adapter card because moving it now would pull manifest dependencies into
`effigy-data` too early.

## Validation

- `git diff --check` passed

## Next Task

Start card
[`512-add-seed-artifact-staging-plan-foundation.md`](./512-add-seed-artifact-staging-plan-foundation.md).
