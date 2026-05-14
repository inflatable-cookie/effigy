# g05.010 - Shared Secrets Vault Access Boundary

Status: Complete
Depends on: `g05.008`
Contract: [`032-secret-and-local-config-management-contract.md`](../../contracts/032-secret-and-local-config-management-contract.md)

## Goal

Converge repeated vault-path, passphrase, and payload-loading behavior behind
one owned support boundary without changing the public secrets model.

## Evidence

- `src/runner/secrets_command.rs`, `src/runner/execute/pipeline/standard.rs`,
  `src/runner/container_command/lifecycle.rs`, and
  `crates/effigy-rhai/src/lib.rs` all reimplement near-identical vault access
  behavior
- `src/runner/secret_session.rs` already owns shared passphrase session logic
- the latest audit identified this as a high-leverage convergence target across
  tasks, containers, Rhai, and local secrets commands

## Scope

- extract shared vault path resolution and payload read/decrypt support
- reuse the existing passphrase session/cache boundary instead of copying it
- keep caller-specific target validation, env projection, and output rendering
  with their current owners
- reduce duplicate-block pressure in secrets-related runtime paths

## Non-Goals

- no secrets manifest redesign
- no backend adapter widening
- no user-facing secrets command changes
- no public test-support surface growth only for convenience

## Acceptance Criteria

- one shared implementation owns vault path and payload loading rules
- task, container, Rhai, and secrets command paths stop reimplementing the same
  access flow
- targeted behavior remains stable across all current secret-consuming surfaces

## Suggested Validation

- targeted secrets command tests
- task secret injection tests
- container secret injection tests
- Rhai secret tests
- `effigy scan duplicate-blocks --json`

## Next Task

Open a card for the first shared support extraction: vault path resolution,
payload loading, and caller-neutral error shaping.
