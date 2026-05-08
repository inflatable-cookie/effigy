# 592 - Close OCI Guides Contracts And Lane Status

Lane: [`060-oci-artifact-closeout-and-proof-matrix-strict-lane.md`](../060-oci-artifact-closeout-and-proof-matrix-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Close the OCI lane cleanly once proof, remediation, and boundary decisions are
done.

## Scope

- sweep guides/help/contracts for stale provisional OCI wording
- make command examples and support claims match the shipped surfaces
- update roadmap/spec front doors so the lane status is obvious
- close `g04.018` only if the earlier cards are actually done

## Non-Goals

- no new OCI behavior
- no broad non-OCI docs cleanup

## Exit Condition

This card is complete when the OCI lane can stop in planning with no ambiguous
“first round” or “later” language left in current active docs unless that
deferment is deliberate and explicit.

## Validation

- docs path checks for touched surfaces
- `git diff --check`

## Next Task

Planning stop.
