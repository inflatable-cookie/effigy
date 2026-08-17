# g08.032 - Vision Governance Operationalization

Status: Complete
Depends on: [`g08.031`](./031-bun-committed-dependency-pinning.md),
[`020-strategic-runway-atlas-v1`](../../vision/020-strategic-runway-atlas-v1.md)
Spec: [`105`](../../specs/archive/105-vision-governance-operationalization-strict-lane.md)

## Goal

Operationalize vision governance so templates `009`, `015`, `017`, and `018`
run on live Effigy data instead of shelfware.

## Vision Alignment

- Primary tags: `MAINT`, `RELEASE`, `OPERATE`
- Target envelope: artifact status, strategic decisions, and governance reviews
  are discoverable, dated, and referenced from planning front doors.
- Vision target delta: maturity baseline `019` advances from template-only to
  first populated governance cycle.

## Goals

- [x] populate artifact status register for vision artifacts `001`–`020`
- [x] create decision record index and seed recent strategic decisions
- [x] publish first governance review log using template `009`
- [x] archive stale strict specs `097`, `099`, and `100`
- [x] close lane without a stale ready card

## Execution Plan

- [x] card `1082`: artifact status register
- [x] card `1083`: decision index and seeded records
- [x] card `1084`: first governance review, logs guidance, archive sweep,
      closeout

## Owner And Seam

Docs owners own governance markdown surfaces. Strict lane `105` owns execution
grammar. Roadmaps and logs reference outcomes; they do not duplicate register
rows.

## Non-Goals

- no automated governance scoring or CI gates in this lane
- no workflow edits
- no release mutation
- no backfill of every historical log with governance sections
- no `g09` rollover

## Acceptance Criteria

- [x] governance front door exists at `docs/vision/governance/README.md`
- [x] register and index link from `docs/vision/README.md`
- [x] three decision records seeded with reversal conditions
- [x] one governance review log in `docs/logs/2026-08/`
- [x] active specs tree no longer carries completed lanes `097`–`100`
- [x] `effigy qa:docs:vision` passes

## Next Task

Lane complete. Run the second governance review on the monthly cadence. Await
operator intent for the next Horizon theme.
