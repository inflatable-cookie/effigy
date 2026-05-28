# 702 - Audit Env Config Secret Boundaries

Roadmap: [`../002-secret-manifest-and-doctor-surface.md`](../002-secret-manifest-and-doctor-surface.md)
Strict lane: pending
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Audit the existing environment/config/secret surfaces before implementing the
`[secrets]` parser.

## Scope

- inspect current `.env.schema` handling
- inspect manifest config parsing seams
- inspect task environment injection behavior
- inspect container environment generation
- inspect Rhai host API environment access
- inspect deploy provider package credential handling
- inspect Underlay and Example App config needs at a classification level
- produce an evidence-backed boundary map for what belongs in:
  - ordinary app config
  - generated Effigy runtime config
  - declared Effigy secrets
  - legacy compatibility paths

## Non-Goals

- no parser implementation
- no vault implementation
- no crypto dependency selection
- no container injection implementation
- no Example App app migration
- no Underlay docs edits unless needed to record a discovered boundary

## Acceptance

- [x] audit identifies concrete files/modules to change in `g05.002`
- [x] audit identifies any `.env.schema` compatibility constraints
- [x] audit identifies where secret redaction already exists and where it is
  missing
- [x] audit records any blocker before parser work
- [x] next card can implement `[secrets]` manifest parsing without rediscovery

## Outcome

Audit recorded in
[`702-env-config-secret-boundary-audit.md`](../audits/702-env-config-secret-boundary-audit.md).

Key result: no blocker for `g05.002`. Add a typed `[secrets]` manifest parser
first, keep `.env.schema` as compatibility, and defer all value storage,
unlock, and injection work.

## Validation

- `rg` and targeted file inspection
- no runtime tests required unless the audit uncovers an existing behavior claim
  that needs proof
- `git diff --check`

## Stop Conditions

- stop if current code already has an incompatible secret backend assumption
- stop if `.env.schema` behavior is too entangled to preserve without a
  separate compatibility roadmap
- stop if Underlay or Example App needs require repo-specific behavior in Effigy

## Next Task

Execute `703` to add the typed `[secrets]` manifest parser and tests.
