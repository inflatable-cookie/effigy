# Contracts Index

This folder contains both:

- machine-consumer JSON contract artifacts used by Effigy command surfaces
- repo-local working rules that govern active strict-lane execution

## Vision Alignment

- Primary tags: `CONTRACT`, `RELEASE`, `MAINT`
- Target envelope: machine-readable contracts stay stable, discoverable, and auditable as commands evolve.
- Vision target delta: contract docs now include explicit ownership and drift-trigger rules instead of relying on implicit process memory.

## Artifacts

- [`001-working-rules.md`](./001-working-rules.md): strict execution rules for
  the active Effigy product lane.
- [`json-schema-index.json`](./json-schema-index.json): canonical schema inventory and validation command mapping.
- [`json-selection-contract.json`](./json-selection-contract.json): CI selection artifact contract used by JSON contract validation flows.

## Contract Ownership and Drift Triggers

| Artifact | Owner | Update triggers | Validation command |
| --- | --- | --- | --- |
| `json-schema-index.json` | Platform maintainers | New JSON command schema, schema version bump, deprecation/removal | `effigy contracts check-json --fast --print-selected` |
| `json-selection-contract.json` | Platform maintainers + CI owner | Selection artifact shape change, validator behavior change | `effigy contracts validate-selection --artifact json-contracts-selected.json` |

## Change Policy

1. Update contract files in the same PR as runtime schema changes.
2. Include a dated log entry in `docs/logs/` when schema or selection shape changes.
3. Include `Vision Target Delta` notes in release/log artifacts for contract-impacting updates.
4. Keep schema IDs/version values additive unless a deliberate compatibility break is documented.

## Next Task

Keep both the machine contracts and the active working-rules contract aligned
to the real validation commands and live execution posture.
