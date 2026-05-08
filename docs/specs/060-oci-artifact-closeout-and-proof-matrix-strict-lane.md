# 060 - OCI Artifact Closeout And Proof Matrix Strict Lane

Roadmap: [`g04.018`](../roadmaps/g04/018-oci-artifact-closeout-and-proof-matrix.md)

Status: Active
Owner: Platform
Created: 2026-05-08

## Purpose

Close the remaining gap between “OCI support exists” and “OCI support is done”.

This lane is about proof, remediation, and contract closeout, not another
round of substrate sprawl.

## Hard Boundaries

- no generic migration framework
- no registry credential UI
- no automatic publish behavior
- no `.github/workflows/` edits
- no release execution
- no broad runtime/container refactors unless they are required to close a
  specific OCI proof or remediation seam

## Current Ready Card

- [`589-add-oci-proof-matrix-for-shipped-surfaces.md`](../roadmaps/g04/batch-cards/589-add-oci-proof-matrix-for-shipped-surfaces.md)

## Execution Chain

- `589` ready: add OCI proof coverage for the shipped bootstrap, container
  data, and artifact surfaces
- `590` pending: harden auth and push failure remediation
- `591` pending: decide and document the artifact operation record boundary
- `592` pending: close OCI docs/contracts/help and mark the lane complete

## Exit Condition

This lane closes when OCI behavior is command-proven, operator-actionable on
failure, and described as a finished product surface rather than a provisional
substrate.

## Next Task

Execute
[`589-add-oci-proof-matrix-for-shipped-surfaces.md`](../roadmaps/g04/batch-cards/589-add-oci-proof-matrix-for-shipped-surfaces.md).
