# 1085 - Align Doctor With Secrets Schema

Roadmap: [`../033-doctor-secrets-schema-parity.md`](../033-doctor-secrets-schema-parity.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)
Spec: [`../../../specs/archive/106-doctor-secrets-schema-parity.md`](../../../specs/archive/106-doctor-secrets-schema-parity.md)

Status: Complete
Owner: `effigy-doctor` manifest schema validation
Created: 2026-08-18
Ready after: operator-selected Bovine health-gate reproduction

## Purpose

Make doctor accept the secret configuration already admitted by the canonical
manifest parser and public contract.

## Work

- add root `secrets` and task `secrets` to the doctor schema allowlists
- validate task secret mode as the existing `required` enum
- add positive contract-shape and negative task-mode regression coverage
- update the changelog
- run focused tests, formatting, Clippy, full QA, and Bovine source-binary proof
- install the corrected local CLI and rerun Bovine doctor

## Acceptance

- [x] representative `[secrets]`, vault, key, and task declarations are clean
- [x] an unsupported task secret mode is rejected
- [x] no secret runtime or parser behavior changes
- [x] full Effigy QA passes
- [x] installed Bovine doctor no longer reports secret keys as unsupported

## Validation

- focused `effigy-doctor` tests
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`
- corrected source and installed binary against the Bovine consumer

## Evidence Requirement

Close with one dated log containing the reproduction, code boundary, focused
and full validation, installed version, and consumer proof.

Evidence:
[`2026-08/18-112147-doctor-secrets-schema-parity-closeout.md`](../../../logs/2026-08/18-112147-doctor-secrets-schema-parity-closeout.md)

## Stop Conditions

Stop on a required public schema change, secret-value access, release mutation,
workflow edit, consumer manifest edit, or unrelated doctor refactor.

## Next Task

Run the second governance review by 2026-09-17. Await operator intent for the
next Horizon theme.
