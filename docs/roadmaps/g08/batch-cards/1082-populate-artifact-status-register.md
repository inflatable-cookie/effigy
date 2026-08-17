# 1082 - Populate Artifact Status Register

Roadmap: [`../032-vision-governance-operationalization.md`](../032-vision-governance-operationalization.md)
Spec: [`../../../specs/archive/105-vision-governance-operationalization-strict-lane.md`](../../../specs/archive/105-vision-governance-operationalization-strict-lane.md)

Status: Complete
Owner: Docs
Created: 2026-08-17
Ready after: operator selected Horizon A Theme 1

## Purpose

Create the first populated artifact status register for vision artifacts `001`
through `020` per spec `017`.

## Work

- add `docs/vision/governance/README.md` front door
- add `docs/vision/governance/artifact-status-register.md` with required fields
- link governance surfaces from `docs/vision/README.md`

## Acceptance

- [x] every indexed vision artifact `001`–`020` has a register row
- [x] rows sorted by numeric ID
- [x] `Superseded` and `Archived` rules respected (none required at first pass)
- [x] register date matches review date

## Validation

- manual review against spec `017`
- `effigy docs check index --policy-index vision`

## Stop Conditions

Stop if register requires inventing artifact states without reading source files.

## Next Task

Execute ready card
[`1083`](./1083-create-decision-index-and-seeded-records.md).
