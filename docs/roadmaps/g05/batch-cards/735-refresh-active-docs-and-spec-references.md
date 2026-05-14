# 735 - Refresh Active Docs And Spec References

Roadmap: [`../015-active-docs-reference-refresh-and-g05-closeout.md`](../015-active-docs-reference-refresh-and-g05-closeout.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Remove dead active spec references and refresh currentness surfaces before final
closeout.

## Completed

- Repointed active references to the archived `010` modularization spec where
  historical context still matters.
- Updated active roadmap/spec front doors so they advertise strict lane `081`
  and the current ready card correctly.
- Left `736` as the final remaining closeout step for the reopened `g05` suite.

## Validation

- `effigy docs check paths docs/audits/reusable-codebase-sweep-prompt.md docs/roadmaps/g04/039-artifact-and-crate-boundary-rejustification.md docs/roadmaps/README.md docs/specs/README.md docs/roadmaps/generation-index.md`
- `effigy docs check links docs/audits/reusable-codebase-sweep-prompt.md docs/roadmaps/g04/039-artifact-and-crate-boundary-rejustification.md docs/roadmaps/README.md docs/specs/README.md docs/roadmaps/generation-index.md`
- `git diff --check`

## Next Task

Execute `736`.
