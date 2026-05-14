# 727 - Switch Container Vault Callers

Roadmap: [`../010-shared-secrets-vault-access-boundary.md`](../010-shared-secrets-vault-access-boundary.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Finish the runner-owned caller migration onto the shared vault-access boundary
for container secret paths.

## Scope

- switch container secret vault path and payload loading to the shared runner
  support
- preserve current container-specific passphrase and env-projection behavior
- leave Rhai adoption for the later crate-local boundary work in `730`

## Completed

- Switched container secret vault path resolution to the shared runner vault
  support.
- Switched container secret payload loading to the shared runner vault support.
- Preserved existing container-specific passphrase and env-projection behavior.

## Next Task

Execute `728`.
