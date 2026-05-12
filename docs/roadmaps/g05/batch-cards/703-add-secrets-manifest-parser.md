# 703 - Add Secrets Manifest Parser

Roadmap: [`../002-secret-manifest-and-doctor-surface.md`](../002-secret-manifest-and-doctor-surface.md)
Strict lane: [`../../../specs/077-secret-manifest-and-doctor-surface-strict-lane.md`](../../../specs/077-secret-manifest-and-doctor-surface-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)
Audit: [`../audits/702-env-config-secret-boundary-audit.md`](../audits/702-env-config-secret-boundary-audit.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Add typed manifest parsing for `[secrets]` without adding secret value storage
or runtime injection.

## Scope

- add `ManifestSecretsConfig`
- add backend selector parsing
- add vault backend config parsing
- add external backend placeholder parsing if needed by the contract
- add key declaration parsing under `[secrets.keys.<name>]`
- add target enum validation
- expose parsed config through `TaskManifest`
- add manifest unit tests for valid and invalid shapes

## Non-Goals

- no `effigy secrets` command implementation
- no vault file creation
- no secret values
- no runtime injection
- no `.env.schema` behavior change

## Acceptance

- [x] valid `[secrets]` config parses
- [x] unknown secret targets fail clearly
- [x] invalid backend values fail clearly
- [x] missing optional fields use documented defaults
- [x] parser exports enough typed data for `704`

## Outcome

- added `ManifestSecretsConfig`
- added backend, vault, external, unlock, identity, key, and target parser
  models
- exposed `TaskManifest.secrets`
- added focused parser tests for valid config, external backend placeholder,
  defaults, invalid backend, invalid target, and full manifest integration

## Validation

- focused `effigy-manifest` tests
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `704` to add read-only `secrets list` and `secrets doctor`.
