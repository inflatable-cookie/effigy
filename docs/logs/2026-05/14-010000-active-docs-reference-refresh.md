# Active Docs Reference Refresh

Date: 2026-05-14

## Summary

Completed card `735`, the active docs/spec reference refresh slice.

## Changes

- repointed active references to the archived `010` modularization spec where
  historical context still matters
- updated active roadmap/spec front doors to show strict lane `081`
- updated current ready work to `736`
- left only the explicit closeout card as remaining ready work in the reopened
  `g05` suite

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Baseline: active docs still pointed at a missing `docs/specs/010...` path and
  one roadmap front door still claimed there was no active strict lane.
- Current state: active docs point at the archived `010` spec or current lane
  surfaces, and front-door currentness matches the live `081` lane.
- Remaining open: final `g05` cleanup-suite closeout.

## Validation

- `effigy docs check paths docs/audits/reusable-codebase-sweep-prompt.md docs/roadmaps/g04/039-artifact-and-crate-boundary-rejustification.md docs/roadmaps/README.md docs/specs/README.md docs/roadmaps/generation-index.md`
- `effigy docs check links docs/audits/reusable-codebase-sweep-prompt.md docs/roadmaps/g04/039-artifact-and-crate-boundary-rejustification.md docs/roadmaps/README.md docs/specs/README.md docs/roadmaps/generation-index.md`
- `git diff --check`

## Next Task

Execute `736` to close the reopened `g05` cleanup suite.
