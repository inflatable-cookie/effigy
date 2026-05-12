# 079 - Task Rhai Deploy Secret Injection Strict Lane

Roadmap: [`g05.004`](../roadmaps/g05/004-task-rhai-and-deploy-secret-injection.md)
Contract: [`032-secret-and-local-config-management-contract.md`](../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Wire declared vault secrets into runtime consumers without writing plaintext
repo files.

This lane starts after the local encrypted vault exists. It may read decrypted
values only after explicit operator unlock/passphrase input and only for
declared target scopes.

## Injection Boundary

Allowed consumers in this lane:

- task execution for `targets = ["tasks"]`
- Rhai scripts for `targets = ["rhai"]`
- deploy provider packages for `targets = ["deploy"]`
- state workflows for `targets = ["state"]`
- artifact workflows for `targets = ["artifacts"]`

Rules:

- no undeclared secret reads
- no target-scope bypass
- no shell command-string interpolation
- no persistent plaintext files
- no compatibility `.env` export
- no container startup injection
- no provider secret provisioning
- missing required target secrets block before side effects
- reports and errors can name keys but must never print values

## Unlock Model

This lane may reuse the current invocation-local passphrase path. It must not
add a daemon or cross-invocation cache.

For non-interactive automation, use test-only harnesses or future explicit
operator-owned integrations. Do not add public plaintext CLI flags.

## Execution Chain

- `712` complete: open this lane and split implementation cards
- `713` complete: task secret injection
- `714` complete: Rhai secret API
- `715` complete: deploy/state/artifact secret injection
- `716` complete: close `g05.004`

## Hard Boundaries

- no container-service startup injection
- no `.env` export
- no provider-hosted secret creation
- no `.env.schema` behavior removal
- no release execution
- no `.github/workflows/` edits

## Acceptance

This lane is complete for task execution, Rhai scripts, deploy provider
packages, and state apply hooks. Artifact-targeted Rhai scope exists for future
artifact script execution points, but built-in artifact commands do not yet
execute Rhai scripts.

## Next Task

Open the first `g05.005` container startup secret injection card.
