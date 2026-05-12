# 717 - Add Container Secret Injection

Roadmap: [`../005-container-secret-injection.md`](../005-container-secret-injection.md)
Strict lane: [`../../../specs/080-container-secret-injection-strict-lane.md`](../../../specs/080-container-secret-injection-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-12

## Purpose

Inject declared container-targeted secrets into local container startup without
committing or persistently writing plaintext `.env` files.

## Scope

- resolve `[secrets]` declarations with `targets = ["containers"]`
- unlock the local vault for container startup when required
- block missing required container secrets before backend mutation
- pass resolved values to container backend startup through process/env APIs
  where possible
- if backend constraints require files, write only under `.effigy/runtime/`
  with explicit lifecycle handling
- redact values from container plan, lifecycle output, JSON reports, and errors
- preserve existing container config and `.env.schema` behavior

## Non-Goals

- no compatibility `.env` export
- no provider-hosted secret creation
- no production secret management
- no Kubernetes, Swarm, or team secret sync
- no automatic app config rewrite

## Acceptance

- `effigy container up` can receive declared container secrets from the vault
- missing required container secrets block before startup
- secret values are not rendered into compose files when avoidable
- any generated plaintext files live only under `.effigy/runtime/`
- values do not appear in container text output, JSON reports, or errors
- existing container tests still pass

## Validation

- container startup planning tests with declared secrets
- missing required container secret blocker tests
- runtime artifact path tests if files are generated
- redaction tests for container reports/errors
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `718` to add explicit compatibility env export.
