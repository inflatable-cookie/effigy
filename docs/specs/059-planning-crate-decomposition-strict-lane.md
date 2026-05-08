# 059 - Planning Crate Decomposition Strict Lane

Roadmap: [`g04.017`](../roadmaps/g04/017-planning-crate-decomposition.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Split the new planning crates by ownership now that their integration seams are
real.

## Hard Boundaries

- no release work
- no `.github/workflows/` edits
- keep public exports stable
- prefer mechanical module moves over behavior edits
- do not split files only to satisfy line counts

## Current Ready Card

No ready card.

## Execution Chain

- `587` complete: split `effigy-container-ops` by operation owner
- `588` complete: extract `effigy-data` tests and close the lane

## Exit Condition

This lane closes when the largest planning crates have clear module owners and
public exports remain stable.

## Next Task

No ready card. The current g04 follow-up set is complete.
