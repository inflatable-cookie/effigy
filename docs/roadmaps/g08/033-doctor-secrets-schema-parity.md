# g08.033 - Doctor Secrets Schema Parity

Status: Complete
Depends on: contract 032 and completed secret-management implementation
Spec: [`106`](../../specs/archive/106-doctor-secrets-schema-parity.md)

## Goal

Restore doctor parity with the already-supported secret manifest surface so
valid consumers can use repository health as a trustworthy gate.

## Vision Alignment

- Primary tags: `CONTRACT`, `MAINT`, `OPERATE`
- Target envelope: doctor agrees with canonical manifest parsing and published
  configuration guidance.
- Vision target delta: supported secret declarations move from false structural
  failure to regression-covered health parity.

## Execution Plan

- [x] card `1085`: align doctor allowlists, add regression coverage, prove the
      Bovine consumer, install the corrected local CLI, and close the lane

## Owner And Seam

`effigy-doctor` owns supplemental raw-manifest diagnostics. The canonical
`effigy-manifest` parser retains schema authority. Contract 032 remains
unchanged.

## Non-Goals

- no vault or task execution behavior change
- no broad doctor/schema consolidation
- no release mutation
- no Bovine manifest change

## Acceptance

- [x] doctor accepts contract-032 root and task secret declarations
- [x] invalid task secret modes remain diagnosed
- [x] focused tests and full QA pass
- [x] the installed local CLI removes the Bovine secrets error

## Evidence

- [`2026-08/18-112147-doctor-secrets-schema-parity-closeout.md`](../../logs/archive/2026-08/18-112147-doctor-secrets-schema-parity-closeout.md)

## Next Task

Run the second governance review by 2026-09-17. Await operator intent for the
next Horizon theme.
