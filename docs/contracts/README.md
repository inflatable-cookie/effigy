# Contracts Index

This folder contains both:

- machine-consumer JSON contract artifacts used by Effigy command surfaces
- repo-local working rules that govern active strict-lane execution

## Vision Alignment

- Primary tags: `CONTRACT`, `RELEASE`, `MAINT`
- Target envelope: machine-readable contracts stay stable, discoverable, and auditable as commands evolve.
- Vision target delta: contract docs now include explicit ownership and drift-trigger rules instead of relying on implicit process memory.

## Active Posture

Active core anchors for the reusable Effigy repo are the provider-neutral and
domain-neutral contracts such as `002`, `019`, `020`, `025`, `027`, `029`,
`030`, `031`, `032`, `033`, `034`, `035`, `036`, `037`, `038`, `039`, and
`040`, plus documentation-graph contract `041` and active external skill-task
contract `042`, plus feature-placement and surface-migration contract `043`.

The older product-specific contracts in this folder remain as historical
evidence and concrete examples. They are not current core ownership anchors for
new reusable-core work.

## Artifacts

- [`001-working-rules.md`](./001-working-rules.md): strict execution rules for
  the active Effigy product lane.
- [`002-production-deployment-model.md`](./002-production-deployment-model.md):
  provider-neutral production deployment contract for the new export surface.
- [`003-underlay-deployment-derivation.md`](./003-underlay-deployment-derivation.md):
  historical first concrete bundle-owned `deploy.model.v1` derivation example
  for the git-hosted `underlay` bundle.
- [`004-underlay-reference-deploy-model-example.md`](./004-underlay-reference-deploy-model-example.md):
  historical first concrete example model for the shipped
  `underlay-reference` repo.
- [`005-container-runtime-contract.md`](./005-container-runtime-contract.md):
  runtime guarantee contract for container-backed task execution, including
  handoff semantics, alias scope, and backend-fallback ownership.
- [`006-compose-backend-compatibility.md`](./006-compose-backend-compatibility.md):
  compose-backend capability matrix for the supported local runtime paths,
  including backend-required versus Effigy-repaired behavior.
- [`007-render-export-contract.md`](./007-render-export-contract.md):
  provider-package export proof contract for Render, defining the bounded
  `render.yaml` mapping used by the external Render package.
- [`008-railway-export-contract.md`](./008-railway-export-contract.md):
  provider-package export proof contract for Railway, defining the bounded
  service-local `railway.toml` plus `report.json` export shape used by the
  external Railway package.
- [`009-execution-surface-convergence.md`](./009-execution-surface-convergence.md):
  common-path convergence contract for repo targeting, binding resolution,
  runtime activation, session ownership, and embedded command re-entry across
  Effigy's execution surfaces.
- [`010-decodelabs-production-strategy.md`](./010-decodelabs-production-strategy.md):
  historical product-boundary contract for Decodelabs, preserving the
  no-fake-automation posture before any future deployment widening.
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
  apply, capture, and Example App/UAT workflows.
- [`015-runtime-operation-pipeline-contract.md`](./015-runtime-operation-pipeline-contract.md):
  runtime operation pipeline contract for execution, activation, container
  operation, and artifact/data request-plan-adapter seams.
- [`016-state-stack-and-layered-seed-framework-contract.md`](./016-state-stack-and-layered-seed-framework-contract.md):
  layered state-stack contract for schema baselines, imported data, overlays,
  lineage, and Example App-style UAT capture/rebase workflows.
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
- [`021-docs-check-subcommand-consolidation-contract.md`](./021-docs-check-subcommand-consolidation-contract.md):
  docs-command consolidation contract for `docs check <KIND>`, removed
  `check-*` spellings, and the unchanged `add-log-index` carveout.
- [`022-command-reference-completeness-and-flag-consistency-contract.md`](./022-command-reference-completeness-and-flag-consistency-contract.md):
  bounded command-reference and repo-targeting contract for `version`
  documentation, missing container shapes/flags, and `--repo` widening on
  changelog and bundle surfaces.
- [`023-container-command-decomposition-contract.md`](./023-container-command-decomposition-contract.md):
  structural-only module-boundary contract for splitting
  `src/runner/container_command/` into cache, volume, lifecycle, data, and
  thin-dispatch owners without widening container behavior.
- [`024-shared-dispatcher-and-exec-collapse-contract.md`](./024-shared-dispatcher-and-exec-collapse-contract.md):
  structural-only duplication-reduction contract for shared json/text result
  rendering, routed container-exec collapse, and shared release-stage control
  flow without changing surfaced behavior.
- [`025-deploy-provider-package-contract.md`](./025-deploy-provider-package-contract.md):
  deploy-provider package contract for moving provider-specific deployment
  behavior into external git/path/OCI packages backed by `provider.toml` and
  Rhai phase scripts.
- [`026-shared-database-target-resolution-contract.md`](./026-shared-database-target-resolution-contract.md):
  shared database target resolution contract for converging seed, dump, state,
  and future migration/media database-service selection behind one domain seam.
- [`027-state-domain-extraction-contract.md`](./027-state-domain-extraction-contract.md):
  state-domain extraction contract for moving pure state report, path, history,
  and planning behavior from the runner into `effigy-state`.
- [`028-manifest-section-decomposition-contract.md`](./028-manifest-section-decomposition-contract.md):
  manifest section decomposition contract for splitting oversized manifest
  parsing files by durable config ownership without grammar drift.
- [`029-deploy-domain-boundary-contract.md`](./029-deploy-domain-boundary-contract.md):
  deploy domain boundary contract for separating transaction models, report
  persistence, provider-package dispatch, and text rendering without schema or
  provider behavior drift.
- [`030-low-risk-deduplication-contract.md`](./030-low-risk-deduplication-contract.md):
  docs-policy test ownership, CLI help topic normalization, private fixture
  builders, and no-behavior-change duplication cleanup rules.
- [`031-artifact-and-crate-boundary-contract.md`](./031-artifact-and-crate-boundary-contract.md):
  artifact refs, staging, OCI, internal module ownership, small-crate retention
  rules, merge-candidate evidence rules, and package-map refresh triggers.
- [`032-secret-and-local-config-management-contract.md`](./032-secret-and-local-config-management-contract.md):
  `g05` secret and local configuration contract covering config/secret
  separation, the built-in human-gated vault posture, secret declarations,
  runtime injection, consumer-repo config conventions, and Varlock adapter
  positioning.
- [`033-gateway-route-table-trust-contract.md`](./033-gateway-route-table-trust-contract.md):
  `g08` gateway route-table trust boundary covering the elevated daemon's
  threat model, read-path integrity verification (ownership/permission +
  managed marker), the fail-closed behavior on an untrusted table, and operator
  visibility in gateway status and doctor.
- [`034-local-dependency-linking-contract.md`](./034-local-dependency-linking-contract.md):
  `g08` machine-local dependency-linking contract covering the `effigy admin deps`
  grammar, Cargo patch and save-less Bun link mechanisms, desired state,
  closure, verification, drift, and lock/manifest hygiene.
- [`035-release-tag-identity-contract.md`](./035-release-tag-identity-contract.md):
  release tag object, deterministic annotation-message, push-order, and
  no-retag evidence contract.
- [`036-papercuts-discovery-contract.md`](./036-papercuts-discovery-contract.md):
  rootless project/collection queue discovery, tolerant Markdown parsing,
  agent-ready JSON, and safe single-project capture contract.
- [`037-explicit-catalog-membership-contract.md`](./037-explicit-catalog-membership-contract.md):
  root-owned catalog membership, named and inline mounted members, shared
  normalization, routing stability, and ambient-discovery removal contract.
- [`038-unified-test-orchestration-contract.md`](./038-unified-test-orchestration-contract.md):
  v0.11 single-authority test configuration, polyglot suite selection,
  non-executing planning, and `tasks.test` removal contract.
- [`039-pre-release-ci-proof-contract.md`](./039-pre-release-ci-proof-contract.md):
  exact-candidate hosted CI evidence required before release preparation and
  execution.
- [`040-bun-committed-dependency-pinning-contract.md`](./040-bun-committed-dependency-pinning-contract.md):
  implemented and consumer-proven root-consumer Bun overrides as a committed
  counterpart to machine-local links.
- [`041-documentation-graph-profile-contract.md`](./041-documentation-graph-profile-contract.md):
  repository-owned documentation graph profiles, exact Markdown semantics,
  bounded context retrieval, and the Northstar runtime-independence boundary.
- [`042-external-skill-task-runner-contract.md`](./042-external-skill-task-runner-contract.md):
  installed task-source and consumer-target separation, isolation, rejection,
  execution, and evidence rules.
- [`043-feature-placement-and-surface-migration-contract.md`](./043-feature-placement-and-surface-migration-contract.md):
  semantic core placement, group-first alias-stable commands, catalog-pack
  simplicity and publication, release/distribution separation, and the S3
  consumer gate.
- [`json-schema-index.json`](./json-schema-index.json): canonical schema inventory and validation command mapping.
- [`json-selection-contract.json`](./json-selection-contract.json): CI selection artifact contract used by JSON contract validation flows.

## Contract Ownership and Drift Triggers

| Artifact | Owner | Update triggers | Validation command |
| --- | --- | --- | --- |
| `005-container-runtime-contract.md` | Platform maintainers | Container-backed handoff semantics, runtime prep ordering, alias guarantee scope, backend fallback ownership | Targeted runtime compatibility tests on the supported local backend path |
| `006-compose-backend-compatibility.md` | Platform maintainers | Supported backend set, backend-required versus repaired capability boundary, named compatibility cases | Targeted runtime compatibility tests on the supported local backend path |
| `009-execution-surface-convergence.md` | Platform maintainers | Execution-surface parity rules, repo-targeting propagation, activation/session ownership, embedded command re-entry semantics | Targeted parity tests across explicit tasks, deferred execution, exec, bootstrap, workspace, and embedded command surfaces |
| `010-decodelabs-production-strategy.md` | Platform maintainers | Historical Decodelabs production boundary, provider-readiness claims, operator-owned production concerns, and future widening target for that product family | Review only when explicitly revisiting Decodelabs-specific deployment planning |
| `011-runtime-context-contract.md` | Platform maintainers | Cwd/root resolution, repo override propagation, boot-time host facts, container handoff marker semantics | `cargo test -p effigy-context` plus targeted runner context tests |
| `012-container-manager-contract.md` | Platform maintainers | Supported backend ids, backend capability boundaries, interrupt/shutdown policy, manager report fields, public report exposure if added | `cargo test -p effigy-containers` plus targeted runner container migration tests |
| `013-task-execution-request-contract.md` | Platform maintainers | Execution request fields, route selection rules, Rhai execution helper behavior, embedded task dispatch behavior, public plan exposure if added | `cargo test -p effigy-execution` plus targeted embedded dispatch parity tests |
| `014-artifact-substrate-contract.md` | Platform maintainers | Artifact ref syntax, metadata schema, OCI pull/push behavior, seed/dump integration, UAT apply/capture semantics, operation ledger fields | Artifact crate tests plus targeted bootstrap/container data seed and dump integration tests |
| `015-runtime-operation-pipeline-contract.md` | Platform maintainers | Pipeline ownership, request/plan/report boundaries, runner adapter boundaries, drift guards, runtime/container proof matrix | `effigy qa:architecture:runtime-container-drift` plus focused runtime/container proof tests |
| `016-state-stack-and-layered-seed-framework-contract.md` | Platform maintainers | Phase taxonomy, stack-manifest shape, lineage boundary, app hook ownership, apply/capture/rebase semantics | Planning review against `g04.019` plus focused state-stack contract proofs once implementation starts |
| `017-task-status-record-and-active-run-model-contract.md` | Platform maintainers | Task-status key fields, normalized state/stage taxonomy, active/completed record layout, stale/live reconciliation rules, covered write-side execution surfaces | Planning review against `g04.020` plus focused task execution and stale-record proofs once implementation starts |
| `018-task-status-query-surface-and-read-model-contract.md` | Platform maintainers | `tasks status` selector resolution rules, `--all` inventory scope, stale/no-longer-declared row visibility, minimum text/JSON fields, read-side ownership split | Planning review against `g04.021` plus focused task-status query proofs once implementation starts |
| `019-deployment-transaction-system-contract.md` | Platform maintainers | Deploy env config, transaction stage order, provider adapter boundary, release/state/artifact composition, deployment reports, redeploy rules, provider support scope | Planning review against `g04.027` through `g04.032` plus focused deployment transaction proofs once implementation starts |
| `020-remote-bundle-sources-git-and-oci-delivery-contract.md` | Platform maintainers | Unified `[bundle].base` grammar, `base_path` removal, source taxonomy, shared materialization boundary, git/OCI cache identity, stale/update detection, `bundle inspect`/`bundle sync` source metadata | Planning review against `g04.022` plus focused bundle parser and source-resolution proofs once implementation starts |
| `021-docs-check-subcommand-consolidation-contract.md` | Platform maintainers | `docs check <KIND>` grammar, removed `check-*` spellings, migration errors, unchanged `add-log-index` carveout, and no-behavior-change rule for underlying checks | Planning review against `g04.023` plus focused docs parser, runner, and help proofs once implementation starts |
| `022-command-reference-completeness-and-flag-consistency-contract.md` | Platform maintainers | Missing command/flag coverage in the command matrix, `version` reference rule, bounded `--repo` widening for `changelog` and `bundle`, and the no-behavior-change rule outside repo targeting | Planning review against `g04.024` plus focused parser, runner, and guide proofs once implementation starts |
| `023-container-command-decomposition-contract.md` | Platform maintainers | Target `container_command` module ownership, structural-only extraction boundary, thin-dispatcher rule for `mod.rs`, and the no-user-facing-change rule during cache/volume/lifecycle splits | Planning review against `g04.025` plus focused container-command proofs once implementation starts |
| `024-shared-dispatcher-and-exec-collapse-contract.md` | Platform maintainers | Shared result-render boundary, routed container-exec collapse scope, release prepare/execute shared-control-flow boundary, and the structural-only no-surface-change rule | Planning review against `g04.026` plus focused runner/output proofs once implementation starts |
| `025-deploy-provider-package-contract.md` | Platform maintainers | Provider package descriptor shape, phase script context/report schema, safety policy, Rhai surface requirements, and the external provider boundary | Planning review before widening provider-package execution beyond Render/Railway proof packages |
| `026-shared-database-target-resolution-contract.md` | Platform maintainers | Database service classification, declared database inventory, selected database calculation, credential reference lookup, and missing/ambiguous target diagnostics across seed, dump, state, and migration/media callers | Planning review against `g04.034` plus focused `effigy-data` and runner seed/dump proofs once implementation starts |
| `027-state-domain-extraction-contract.md` | Platform maintainers | State report/path/history/planning ownership, runner side-effect boundary, output compatibility, and future media/object-store state seam readiness | Planning review against `g04.035` plus focused `effigy-state` and state command proofs once implementation starts |
| `028-manifest-section-decomposition-contract.md` | Platform maintainers | Manifest section module ownership, public API compatibility, parse error compatibility, and no-grammar-drift rule for bundle, state, deploy, object-store, container, root, and import config parsing | Planning review against `g04.036` plus focused manifest parser and composition tests once implementation starts |
| `029-deploy-domain-boundary-contract.md` | Platform maintainers | Deploy transaction ownership, report persistence paths, provider-package dispatch context, text rendering boundary, JSON schema compatibility, and provider-specific behavior staying outside core | Planning review against `g04.037` plus focused deploy transaction and provider package fixture tests once implementation starts |
| `030-low-risk-deduplication-contract.md` | Platform maintainers | Docs-policy test ownership, CLI help topic normalization, private fixture-builder boundaries, and no-behavior-change duplication cleanup rules | Planning review against `g04.038` plus focused docs-policy, help, fixture, and duplicate-block scan proofs once implementation starts |
| `031-artifact-and-crate-boundary-contract.md` | Platform maintainers | Artifact refs/staging/OCI/module ownership, small-crate retention rules, merge-candidate evidence rules, and package-map refresh triggers | Planning review against `g04.039` plus artifact tests, crate-boundary docs review, god-file scan, and cargo check once implementation starts |
| `032-secret-and-local-config-management-contract.md` | Platform maintainers | `[secrets]` manifest shape, built-in vault unlock policy, redaction rules, task/container/Rhai/deploy injection, `.env.schema` relationship, consumer-repo config convention, and Varlock adapter posture | Planning review against `g05.001` plus focused secrets, vault, injection, redaction, container, Rhai, and deploy-provider tests once implementation starts |
| `033-gateway-route-table-trust-contract.md` | Platform maintainers | Gateway route-table trust boundary, read-path integrity mechanism (ownership/permission + managed marker), fail-closed failure mode, and operator visibility in gateway status/doctor | Planning review against `g08.014` plus focused trust-verification fixtures (well-formed, tampered, wrong-permission, foreign-marked) once implementation starts |
| `034-local-dependency-linking-contract.md` | Platform maintainers | `effigy admin deps` grammar, Cargo/Bun mechanism behavior, closure rules, desired-state schema/location, manifest/lock invariants, doctor severity, and JSON payload shape | Planning review against `g08.018` through `g08.023`; focused manager, state, doctor, and portfolio proofs once implementation starts |
| `035-release-tag-identity-contract.md` | Platform maintainers | Release tag object type, annotation-message derivation, signing posture, tag push ordering, or no-retag evidence | Focused `effigy-release` tests plus execute-success local and bare-remote tag-object proof |
| `036-papercuts-discovery-contract.md` | Platform maintainers | Papercut Markdown convention, scope rules, parser diagnostics, command grammar, JSON payload, or capture safety | `cargo test -p effigy-papercuts` plus focused CLI and command-output tests |
| `037-explicit-catalog-membership-contract.md` | Platform maintainers | Catalog member grammar, structured system mounts, normalization, routing membership, discovery removal, or membership diagnostics/JSON evidence | Focused manifest, routing, container, doctor, CLI, test-plan, JSON, and consumer-shape proofs |
| `038-unified-test-orchestration-contract.md` | Platform maintainers | Test selector precedence, `[test]` grammar, suite selection, plan safety, migration, or supported ecosystem detection | Focused manifest, built-in test, migration, runner, docs, and JSON contract proofs |
| `039-pre-release-ci-proof-contract.md` | Platform maintainers | Exact-candidate hosted CI identity, accepted trigger/branch/conclusion, release gate ordering, or checker ownership | Focused checker fixtures plus release-gate configuration and protocol review |
| `040-bun-committed-dependency-pinning-contract.md` | Platform maintainers | Pin/unpin grammar, closure selection, override conflict policy, manifest write safety, path portability, link interaction, or JSON payload shape | Focused deps, CLI, runner, JSON, and Soundcheck/Poodle consumer proofs |
| `041-documentation-graph-profile-contract.md` | Platform maintainers | Profile grammar, section boundaries, currentness, authority ranking, context budgets, JSON shape, freshness identity, or Northstar runtime independence | Planning review against `g08.035`; focused manifest, codegraph, CLI, docs, JSON, generic-fixture, and Northstar-starter proofs during implementation |
| `042-external-skill-task-runner-contract.md` | Platform maintainers | Skill source/consumer target separation, isolated task loading, path classes, nested dispatch, rejection boundaries, or JSON evidence | Planning review against `g08.037`; focused CLI, context, manifest, routing, execution, Rhai, JSON, docs, and Northstar-skill proofs |
| `043-feature-placement-and-surface-migration-contract.md` | Product architecture and platform maintainers | Core placement criteria, command grouping or alias policy, repository-intelligence ownership, catalog-pack UX/source/support/publication/update rules, release/distribution ownership, or S3 consumer migration state | Archived strict spec `115`; focused parity, compatibility, deterministic artifact, offline, provenance, release-safety, docs, JSON, and full Effigy QA during implementation |
| `044-rhai-storage-create-only-contract.md` | Platform maintainers | Atomic create-if-absent semantics for Rhai object storage, compatibility, collision diagnostics, and redaction | Focused `effigy-rhai` request/collision fixtures plus full Effigy QA |
| `json-schema-index.json` | Platform maintainers | New JSON command schema, schema version bump, deprecation/removal | `effigy repo contracts check-json --fast --print-selected` |
| `json-selection-contract.json` | Platform maintainers + CI owner | Selection artifact shape change, validator behavior change | `effigy repo contracts validate-selection --artifact json-contracts-selected.json` |

## Change Policy

1. Update contract files in the same PR as runtime schema changes.
2. Include a dated log entry in `docs/logs/` when schema or selection shape changes.
3. Include `Vision Target Delta` notes in release/log artifacts for contract-impacting updates.
4. Keep schema IDs/version values additive unless a deliberate compatibility break is documented.

## Retained Contract Posture

Keep both the machine contracts and the active working-rules contract aligned
to the real validation commands and live execution posture, and use
`002-production-deployment-model.md` as the active provider-neutral deploy
model anchor, and treat `003-underlay-deployment-derivation.md`,
`004-underlay-reference-deploy-model-example.md`,
`007-render-export-contract.md`, `008-railway-export-contract.md`, and
`010-decodelabs-production-strategy.md` as retained historical or example
evidence rather than current reusable-core anchors, while keeping
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
remote bundle-source contract, and
`021-docs-check-subcommand-consolidation-contract.md` as the `g04.023`
docs-check consolidation contract, and
`022-command-reference-completeness-and-flag-consistency-contract.md` as the
`g04.024` command-reference and repo-targeting contract, and
`023-container-command-decomposition-contract.md` as the `g04.025`
container-command decomposition contract, and
`024-shared-dispatcher-and-exec-collapse-contract.md` as the `g04.026`
shared dispatcher and exec collapse contract, `029-deploy-domain-boundary-contract.md`
as the `g04.037` deploy domain boundary contract, and
`030-low-risk-deduplication-contract.md` as the `g04.038` low-risk
deduplication contract, `031-artifact-and-crate-boundary-contract.md` as the
`g04.039` artifact and crate-boundary contract, and
`032-secret-and-local-config-management-contract.md` as the active `g05.001`
secret and local configuration contract, and
`037-explicit-catalog-membership-contract.md` as the explicit catalog
membership boundary, `038-unified-test-orchestration-contract.md` as the
active v0.11 test authority and plan-safety boundary, and
`039-pre-release-ci-proof-contract.md` as the exact-candidate hosted CI
release boundary, and `040-bun-committed-dependency-pinning-contract.md` as the
implemented committed Bun override boundary distinct from machine-local
linking, and `041-documentation-graph-profile-contract.md` as the active
repository-defined documentation graph and bounded retrieval boundary, and
`042-external-skill-task-runner-contract.md` as the explicit external task
source and consumer runtime target boundary, and
`043-feature-placement-and-surface-migration-contract.md` as the semantic core,
grouped-command preview, provider/asset placement, and migration-gate boundary,
and `044-rhai-storage-create-only-contract.md` as the retained Rhai storage
exclusive-create boundary.

## Next Task

The grouped-command preview shipped (card `1109`; strict spec `116` archived).
Direct-route removal remains blocked on the explicit `v1.0` gate with
refreshed consumer evidence; Effigy release authority stays separate.
