# 106 - Doctor Secrets Schema Parity

Status: Complete
Owner: `effigy-doctor` manifest schema validation
Roadmap: [`g08.033`](../../roadmaps/g08/033-doctor-secrets-schema-parity.md)
Contracts: [`001`](../../contracts/001-working-rules.md),
[`032`](../../contracts/032-secret-and-local-config-management-contract.md)

## Problem

The canonical manifest parser, schema reference, secrets guide, and runtime
accept root `[secrets]` declarations and task `secrets = "required"`. Doctor's
separate raw-schema allowlists omitted both, so valid consumer manifests
received a structural error and could not reach a clean health gate.

## Decision

- Add `secrets` to doctor's top-level manifest allowlist.
- Add `secrets` to doctor's task-table allowlist and validate its only admitted
  value, `required`.
- Cover one representative manifest containing vault configuration, declared
  task-target keys, and a managed secret-required task.
- Prove the affected Bovine consumer with the corrected source and installed
  binaries.

The strict canonical parser continues to own nested secret shape validation.
This lane did not duplicate that parser inside doctor.

## Non-Goals

- no secrets runtime, vault, injection, or redaction change
- no general manifest-schema refactor
- no compatibility alias or new secret mode
- no release preparation, tag, workflow, or consumer manifest edit

## Acceptance

- [x] valid root and task secret declarations produce no doctor schema finding
- [x] unsupported task secret modes still produce an explicit schema finding
- [x] focused doctor tests pass
- [x] full Effigy QA passes
- [x] corrected installed CLI removes Bovine's secret-key doctor error

## Evidence

- [`2026-08/18-112147-doctor-secrets-schema-parity-closeout.md`](../../logs/2026-08/18-112147-doctor-secrets-schema-parity-closeout.md)

## Next Task

Run the second governance review by 2026-09-17. Await operator intent for the
next Horizon theme.
