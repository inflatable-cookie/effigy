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
- [`002-production-deployment-model.md`](./002-production-deployment-model.md):
  provider-neutral production deployment contract for the new export surface.
- [`003-underlay-deployment-derivation.md`](./003-underlay-deployment-derivation.md):
  first concrete mapping from the shipped `underlay` bundle into
  `deploy.model.v1`.
- [`004-underlay-reference-deploy-model-example.md`](./004-underlay-reference-deploy-model-example.md):
  first concrete example model for the shipped `underlay-reference` repo.
- [`005-container-runtime-contract.md`](./005-container-runtime-contract.md):
  runtime guarantee contract for container-backed task execution, including
  handoff semantics, alias scope, and backend-fallback ownership.
- [`006-compose-backend-compatibility.md`](./006-compose-backend-compatibility.md):
  compose-backend capability matrix for the supported local runtime paths,
  including backend-required versus Effigy-repaired behavior.
- [`007-render-export-contract.md`](./007-render-export-contract.md):
  first provider-export contract for the managed deployment lane, defining the
  bounded `render.yaml` mapping.
- [`008-railway-export-contract.md`](./008-railway-export-contract.md):
  second provider-export contract for the managed deployment lane, defining the
  first bounded service-local `railway.toml` plus `report.json` export shape.
- [`json-schema-index.json`](./json-schema-index.json): canonical schema inventory and validation command mapping.
- [`json-selection-contract.json`](./json-selection-contract.json): CI selection artifact contract used by JSON contract validation flows.

## Contract Ownership and Drift Triggers

| Artifact | Owner | Update triggers | Validation command |
| --- | --- | --- | --- |
| `005-container-runtime-contract.md` | Platform maintainers | Container-backed handoff semantics, runtime prep ordering, alias guarantee scope, backend fallback ownership | Targeted runtime compatibility tests on the supported local backend path |
| `006-compose-backend-compatibility.md` | Platform maintainers | Supported backend set, backend-required versus repaired capability boundary, named compatibility cases | Targeted runtime compatibility tests on the supported local backend path |
| `json-schema-index.json` | Platform maintainers | New JSON command schema, schema version bump, deprecation/removal | `effigy contracts check-json --fast --print-selected` |
| `json-selection-contract.json` | Platform maintainers + CI owner | Selection artifact shape change, validator behavior change | `effigy contracts validate-selection --artifact json-contracts-selected.json` |

## Change Policy

1. Update contract files in the same PR as runtime schema changes.
2. Include a dated log entry in `docs/logs/` when schema or selection shape changes.
3. Include `Vision Target Delta` notes in release/log artifacts for contract-impacting updates.
4. Keep schema IDs/version values additive unless a deliberate compatibility break is documented.

## Next Task

Keep both the machine contracts and the active working-rules contract aligned
to the real validation commands and live execution posture, and use
`002-production-deployment-model.md` plus
`003-underlay-deployment-derivation.md` and
`004-underlay-reference-deploy-model-example.md` as the contract anchors for
`g03.001`, plus `007-render-export-contract.md` as the first provider-adapter
contract anchor for the same lane, plus
`008-railway-export-contract.md` as the second provider-adapter contract
anchor for the same lane, and `005-container-runtime-contract.md` as the
contract anchor for the `g03.004` to `g03.006` runtime-hardening lane, with
`006-compose-backend-compatibility.md` defining the active backend capability
matrix for `g03.006`.
