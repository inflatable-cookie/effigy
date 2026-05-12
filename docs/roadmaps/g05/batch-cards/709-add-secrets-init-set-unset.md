# 709 - Add Secrets Init Set Unset

Roadmap: [`../003-local-encrypted-vault.md`](../003-local-encrypted-vault.md)
Strict lane: [`../../../specs/078-local-encrypted-vault-strict-lane.md`](../../../specs/078-local-encrypted-vault-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Add the first vault mutation commands.

## Scope

- add `effigy secrets init`
- add `effigy secrets set <name>`
- add `effigy secrets unset <name>`
- require interactive human input for passphrases and values
- reject undeclared secret names
- write vault files with safe permissions
- keep all command output value-free

## Non-Goals

- no runtime injection
- no `.env` export
- no non-interactive production value flags
- no provider secret provisioning

## Acceptance

- [x] init creates an empty vault at `[secrets.vault].path`
- [x] set stores declared key values
- [x] unset removes declared key values
- [x] commands fail clearly when `[secrets]` or `[secrets.vault]` is missing
- [x] secret values never appear in text, JSON, errors, or debug output

## Outcome

Added `effigy secrets init`, `effigy secrets set <name>`, and
`effigy secrets unset <name>`. The commands require declared keys, use hidden
interactive input in normal CLI operation, write encrypted vault files with
safe Unix permissions, and keep all output value-free.

## Validation

- CLI parser tests
- runner command tests with test-only prompt harnesses
- file permission tests where supported
- redaction tests
- `cargo check --all-targets`
- `git diff --check`

## Next Task

Execute `710` to add unlock/lock and doctor vault diagnostics.
