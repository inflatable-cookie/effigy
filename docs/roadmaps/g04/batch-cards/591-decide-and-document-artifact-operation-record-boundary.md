# 591 - Decide And Document Artifact Operation Record Boundary

Lane: [`060-oci-artifact-closeout-and-proof-matrix-strict-lane.md`](../060-oci-artifact-closeout-and-proof-matrix-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Settle whether the current artifact operation record/ledger language is part of
the shipped OCI contract now or explicitly deferred.

## Scope

- inspect current contract and guide language around:
  - apply/capture reports
  - operation ledger wording
  - environment/digest/task/timestamp expectations
- decide:
  - shipped contract now
  - or deferred follow-up with explicit boundary
- align docs/contracts/changelog with that decision

## Non-Goals

- no full ledger subsystem implementation unless the current product already
  accidentally depends on it
- no database-level migration history work

## Exit Condition

This card is complete when operators and maintainers can tell, from current
docs alone, whether artifact operation records are a finished supported surface
or an intentionally deferred one.

## Validation

- docs path checks for touched contract/guide surfaces
- `git diff --check`

## Next Task

Continue with
[`592-close-oci-guides-contracts-and-lane-status.md`](./592-close-oci-guides-contracts-and-lane-status.md).
