# 594 - Promote Phase Model Stack Manifest And Acowtancy Proof Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Turn the first draft state-stack contract into a settled implementation-ready
planning boundary.

## Scope

- lock the phase taxonomy Effigy will recognize in the first shipped stack model
- define the minimum manifest fields and report fields needed for apply/capture
- decide how stack lineage relates to the existing artifact operation record
- pin the Acowtancy proof loop to one bounded first implementation slice
- identify what must stay app-owned at the hook boundary

## Non-Goals

- no CLI parser implementation
- no durable ledger implementation yet
- no automatic sync or reconciliation behavior
- no app-specific merge rules
- no release work

## Exit Condition

This card is complete when the contract is specific enough that the first
implementation card can be chosen without reopening phase semantics or
Acowtancy ownership boundaries.

## Closeout

- promoted `016-state-stack-and-layered-seed-framework-contract.md` to active
- locked the first recognized phase model
- defined minimum stack, layer, apply-report, capture-report, and lineage
  fields
- decided that state-stack lineage rolls up artifact operation reports rather
  than replacing them
- bounded the first Acowtancy proof to manifest validation and lineage planning
  without Farmyard hook execution

## Validation

- PASS: docs path checks for changed planning docs
- PASS: `git diff --check`

## Next Task

Card
[`595-implement-state-stack-manifest-and-lineage-plan-foundation.md`](./595-implement-state-stack-manifest-and-lineage-plan-foundation.md).
