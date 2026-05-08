# 590 - Harden OCI Auth And Push Failure Remediation

Lane: [`060-oci-artifact-closeout-and-proof-matrix-strict-lane.md`](../060-oci-artifact-closeout-and-proof-matrix-strict-lane.md)

Status: In Progress
Owner: Platform
Created: 2026-05-08

## Goal

Make failed OCI inspect/pull/push operations tell operators what to do next,
not just which adapter path failed.

## Scope

- audit current OCI failure text for:
  - missing `oras`
  - not logged in / auth failure
  - denied push
  - invalid digest-pinned push destination
  - unreachable or malformed registry refs
- tighten text and JSON hints around the real remediation path
- add focused tests for the operator-facing failure contract

## Non-Goals

- no credential store management
- no retry orchestration
- no background auth probing

## Exit Condition

This card is complete when the main OCI failure classes produce clear
operator-facing remediation and those messages are contract-tested.

## Validation

- targeted artifact/runner tests for OCI failure rendering
- docs path checks for any touched guides/contracts
- `git diff --check`

## Next Task

Continue with
[`591-decide-and-document-artifact-operation-record-boundary.md`](./591-decide-and-document-artifact-operation-record-boundary.md).
