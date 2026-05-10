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
- [`009-execution-surface-convergence.md`](./009-execution-surface-convergence.md):
  common-path convergence contract for repo targeting, binding resolution,
  runtime activation, session ownership, and embedded command re-entry across
  Effigy's execution surfaces.
- [`010-decodelabs-production-strategy.md`](./010-decodelabs-production-strategy.md):
  the short-term production-truth contract for Decodelabs, defining the
  no-fake-automation boundary before any future deployment widening.
- [`011-runtime-context-contract.md`](./011-runtime-context-contract.md):
  boot-time runtime context contract for cwd, repo target, host facts, and
  container handoff state.
- [`012-container-manager-contract.md`](./012-container-manager-contract.md):
  manager-facade contract for backend selection, container operations, and
  interrupt-aware closeout.
- [`013-task-execution-request-contract.md`](./013-task-execution-request-contract.md):
  canonical request/plan contract for direct, embedded, Rhai, deferral, demo,
  and managed task execution.
- [`014-artifact-substrate-contract.md`](./014-artifact-substrate-contract.md):
  standalone artifact contract for local and OCI data payloads used by seed,
  apply, capture, and Acowtancy/UAT workflows.
- [`015-runtime-operation-pipeline-contract.md`](./015-runtime-operation-pipeline-contract.md):
  runtime operation pipeline contract for execution, activation, container
  operation, and artifact/data request-plan-adapter seams.
- [`016-state-stack-and-layered-seed-framework-contract.md`](./016-state-stack-and-layered-seed-framework-contract.md):
  layered state-stack contract for schema baselines, imported data, overlays,
  lineage, and Acowtancy-style UAT capture/rebase workflows.
- [`017-task-status-record-and-active-run-model-contract.md`](./017-task-status-record-and-active-run-model-contract.md):
  task-status record contract for identity, normalized status/stage taxonomy,
  active/completed persistence, and stale-record reconciliation before the
  later read/query lane.
- [`018-task-status-query-surface-and-read-model-contract.md`](./018-task-status-query-surface-and-read-model-contract.md):
  task-status query contract for selector resolution, repo-plus-descendant
  inventory scope, stale-row visibility, and minimum text/JSON read-side
  result shape.
- [`019-deployment-transaction-system-contract.md`](./019-deployment-transaction-system-contract.md):
  v0.6.0 deployment transaction contract for provider-neutral UAT and
  production deployment orchestration across code refs, state stacks, OCI
  artifacts, release evidence, provider adapters, hooks, reports, and redeploy.
- [`020-remote-bundle-sources-git-and-oci-delivery-contract.md`](./020-remote-bundle-sources-git-and-oci-delivery-contract.md):
  unified bundle-source contract for shipped, path, git, and OCI delivery,
  `base_path` removal, shared source materialization, cache identity, and
  stale/update detection.
- [`json-schema-index.json`](./json-schema-index.json): canonical schema inventory and validation command mapping.
- [`json-selection-contract.json`](./json-selection-contract.json): CI selection artifact contract used by JSON contract validation flows.

## Contract Ownership and Drift Triggers

| Artifact | Owner | Update triggers | Validation command |
| --- | --- | --- | --- |
| `005-container-runtime-contract.md` | Platform maintainers | Container-backed handoff semantics, runtime prep ordering, alias guarantee scope, backend fallback ownership | Targeted runtime compatibility tests on the supported local backend path |
| `006-compose-backend-compatibility.md` | Platform maintainers | Supported backend set, backend-required versus repaired capability boundary, named compatibility cases | Targeted runtime compatibility tests on the supported local backend path |
| `009-execution-surface-convergence.md` | Platform maintainers | Execution-surface parity rules, repo-targeting propagation, activation/session ownership, embedded command re-entry semantics | Targeted parity tests across explicit tasks, deferred execution, exec, bootstrap, workspace, and embedded command surfaces |
| `010-decodelabs-production-strategy.md` | Platform maintainers | Decodelabs production boundary, provider-readiness claims, operator-owned production concerns, future widening target | Planning review against `g03.003` plus any future Decodelabs deploy-surface proofs |
| `011-runtime-context-contract.md` | Platform maintainers | Cwd/root resolution, repo override propagation, boot-time host facts, container handoff marker semantics | `cargo test -p effigy-context` plus targeted runner context tests |
| `012-container-manager-contract.md` | Platform maintainers | Supported backend ids, backend capability boundaries, interrupt/shutdown policy, manager report fields, public report exposure if added | `cargo test -p effigy-containers` plus targeted runner container migration tests |
| `013-task-execution-request-contract.md` | Platform maintainers | Execution request fields, route selection rules, Rhai execution helper behavior, embedded task dispatch behavior, public plan exposure if added | `cargo test -p effigy-execution` plus targeted embedded dispatch parity tests |
| `014-artifact-substrate-contract.md` | Platform maintainers | Artifact ref syntax, metadata schema, OCI pull/push behavior, seed/dump integration, UAT apply/capture semantics, operation ledger fields | Artifact crate tests plus targeted bootstrap/container data seed and dump integration tests |
| `015-runtime-operation-pipeline-contract.md` | Platform maintainers | Pipeline ownership, request/plan/report boundaries, runner adapter boundaries, drift guards, runtime/container proof matrix | `bash scripts/check-runtime-container-drift.sh` plus focused runtime/container proof tests |
| `016-state-stack-and-layered-seed-framework-contract.md` | Platform maintainers | Phase taxonomy, stack-manifest shape, lineage boundary, app hook ownership, apply/capture/rebase semantics | Planning review against `g04.019` plus focused state-stack contract proofs once implementation starts |
| `017-task-status-record-and-active-run-model-contract.md` | Platform maintainers | Task-status key fields, normalized state/stage taxonomy, active/completed record layout, stale/live reconciliation rules, covered write-side execution surfaces | Planning review against `g04.020` plus focused task execution and stale-record proofs once implementation starts |
| `018-task-status-query-surface-and-read-model-contract.md` | Platform maintainers | `tasks status` selector resolution rules, `--all` inventory scope, stale/no-longer-declared row visibility, minimum text/JSON fields, read-side ownership split | Planning review against `g04.021` plus focused task-status query proofs once implementation starts |
| `019-deployment-transaction-system-contract.md` | Platform maintainers | Deploy env config, transaction stage order, provider adapter boundary, release/state/artifact composition, deployment reports, redeploy rules, provider support scope | Planning review against `g04.027` through `g04.032` plus focused deployment transaction proofs once implementation starts |
| `020-remote-bundle-sources-git-and-oci-delivery-contract.md` | Platform maintainers | Unified `[bundle].base` grammar, `base_path` removal, source taxonomy, shared materialization boundary, git/OCI cache identity, stale/update detection, `bundle inspect`/`bundle sync` source metadata | Planning review against `g04.022` plus focused bundle parser and source-resolution proofs once implementation starts |
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
anchor for the same lane, `010-decodelabs-production-strategy.md` as the
contract anchor for the active `g03.003` Decodelabs planning lane,
`005-container-runtime-contract.md` as the contract anchor for the `g03.004`
to `g03.006` runtime-hardening lane, `006-compose-backend-compatibility.md`
as the backend capability matrix for `g03.006`,
`009-execution-surface-convergence.md` as the convergence contract, and
`011-runtime-context-contract.md` as the `g03.030` context contract,
`012-container-manager-contract.md` as the `g03.031` manager contract, and
`013-task-execution-request-contract.md` as the `g03.032` execution request
contract, `014-artifact-substrate-contract.md` as the `g03.036` artifact
substrate contract, and `015-runtime-operation-pipeline-contract.md` as the
`g04` runtime operation pipeline contract, and
`016-state-stack-and-layered-seed-framework-contract.md` as the `g04.019`
state-stack framework contract, and
`017-task-status-record-and-active-run-model-contract.md` as the `g04.020`
task-status record contract, and
`018-task-status-query-surface-and-read-model-contract.md` as the `g04.021`
task-status query contract, and
`019-deployment-transaction-system-contract.md` as the `g04.027` to `g04.032`
deployment transaction contract, and
`020-remote-bundle-sources-git-and-oci-delivery-contract.md` as the `g04.022`
remote bundle-source contract.
