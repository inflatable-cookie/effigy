# 726 - Add Shared Runner Vault Access Support

Roadmap: [`../010-shared-secrets-vault-access-boundary.md`](../010-shared-secrets-vault-access-boundary.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Add the first shared vault path and payload-loading support and switch the
lowest-risk runner callers.

## Completed

- Added runner-owned shared vault path and payload-loading helpers in
  `secret_vault.rs`.
- Switched local vault command and task secret callers to the shared support
  boundary.
- Kept backend validation and caller-specific projection logic with the current
  owners.

## Next Task

Execute `727` to migrate the container caller onto the same shared support.
