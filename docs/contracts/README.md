# Contracts Index

This folder contains machine-consumer JSON contract artifacts used by Effigy command surfaces.

## Vision Alignment

- Primary tags: `CONTRACT`, `RELEASE`, `MAINT`
- Target envelope: machine-readable contracts stay stable, discoverable, and auditable as commands evolve.
- Vision target delta: contract docs now include explicit ownership and drift-trigger rules instead of relying on implicit process memory.

## Artifacts

- [`json-schema-index.json`](./json-schema-index.json): canonical schema inventory and validation command mapping.
- [`json-selection-contract.json`](./json-selection-contract.json): CI selection artifact contract used by JSON contract validation flows.

## Contract Ownership and Drift Triggers

| Artifact | Owner | Update triggers | Validation command |
| --- | --- | --- | --- |
| `json-schema-index.json` | Platform maintainers | New JSON command schema, schema version bump, deprecation/removal | `./scripts/check-json-contracts.sh --fast --print-selected=text` |
| `json-selection-contract.json` | Platform maintainers + CI owner | Selection artifact shape change, validator behavior change | `./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json` |

## Change Policy

1. Update contract files in the same PR as runtime schema changes.
2. Include a dated report entry in `docs/reports/` when schema or selection shape changes.
3. Include `Vision Target Delta` notes in release/report artifacts for contract-impacting updates.
4. Keep schema IDs/version values additive unless a deliberate compatibility break is documented.

## Next Task

Add docs QA automation that fails when contract index ownership/trigger references are removed or drift from current validation commands.
