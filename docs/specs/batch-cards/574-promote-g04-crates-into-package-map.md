# 574 - Promote g04 Crates Into Package Map

Lane: [`053-contract-promotion-and-g04-closeout-strict-lane.md`](../053-contract-promotion-and-g04-closeout-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Update the live package map so it names the shipped `g04` runtime/container
ownership boundaries.

## Scope

- add `effigy-runtime-plan`, `effigy-container-ops`, `effigy-data`, and
  `effigy-artifacts` to the workspace crate map
- update runner/runtime ownership notes where `g04` moved planning authority
  out of runner glue
- keep this to architecture ownership, not contract text
- select the first contract promotion card after the map is current

## Non-Goals

- no contract rewrites in this card
- no code changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `docs/architecture/010-package-map.md` reflects the
current `g04` crates and module ownership seams.

## Validation

- docs path/link check for the package map if available
- `git diff --check`

## Next Task

Start
[`575-add-runtime-operation-pipeline-contract.md`](./575-add-runtime-operation-pipeline-contract.md).
