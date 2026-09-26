# Contracts

Current technical authority for Effigy. Each contract owns a specific behavior
or compatibility boundary. The [release procedure](release.md) owns release
steps; [writing style](writing-style.md) owns internal prose. Machine-readable
schema artifacts live beside these files.

| Contract | Scope |
| --- | --- |
| [002-production-deployment-model.md](002-production-deployment-model.md) | provider-neutral production deployment contract for the new export surface. |
| [003-underlay-deployment-derivation.md](003-underlay-deployment-derivation.md) | Underlay bundle-owned service, domain, backing-service, secret, warning, and omission rules. |
| [004-underlay-reference-deploy-model-example.md](004-underlay-reference-deploy-model-example.md) | Current Underlay Reference inputs and model declaration; the former JSON specimen is not live output. |
| [005-container-runtime-contract.md](005-container-runtime-contract.md) | runtime guarantee contract for container-backed task execution, including handoff semantics, alias scope, and backend-fallback ownership. |
| [006-compose-backend-compatibility.md](006-compose-backend-compatibility.md) | compose-backend capability matrix for the supported local runtime paths, including backend-required versus Effigy-repaired behavior. |
| [007-render-export-contract.md](007-render-export-contract.md) | provider-package export proof contract for Render, defining the bounded `render.yaml` mapping used by the external Render package. |
| [008-railway-export-contract.md](008-railway-export-contract.md) | provider-package export proof contract for Railway, defining the bounded service-local `railway.toml` plus `report.json` export shape used by the external Railway package. |
| [009-execution-surface-convergence.md](009-execution-surface-convergence.md) | common-path convergence contract for repo targeting, binding resolution, runtime activation, session ownership, and embedded command re-entry across Effigy's execution surfaces. |
| [010-decodelabs-production-strategy.md](010-decodelabs-production-strategy.md) | Decodelabs local-development versus unproved production-export boundary and operator-owned concerns. |
| [011-runtime-context-contract.md](011-runtime-context-contract.md) | boot-time runtime context contract for cwd, repo target, host facts, and container handoff state. |
| [012-container-manager-contract.md](012-container-manager-contract.md) | manager-facade contract for backend selection, container operations, and interrupt-aware closeout. |
| [013-task-execution-request-contract.md](013-task-execution-request-contract.md) | canonical request/plan contract for direct, embedded, Rhai, deferral, demo, and managed task execution. |
| [014-artifact-substrate-contract.md](014-artifact-substrate-contract.md) | standalone artifact contract for local and OCI data payloads used by seed, apply, capture, and Example App/UAT workflows. |
| [015-runtime-operation-pipeline-contract.md](015-runtime-operation-pipeline-contract.md) | runtime operation pipeline contract for execution, activation, container operation, and artifact/data request-plan-adapter seams. |
| [016-state-stack-and-layered-seed-framework-contract.md](016-state-stack-and-layered-seed-framework-contract.md) | layered state-stack contract for schema baselines, imported data, overlays, lineage, and Example App-style UAT capture/rebase workflows. |
| [017-task-status-record-and-active-run-model-contract.md](017-task-status-record-and-active-run-model-contract.md) | task-status record contract for identity, normalized status/stage taxonomy, active/completed persistence, and stale-record reconciliation before the later read/query lane. |
| [018-task-status-query-surface-and-read-model-contract.md](018-task-status-query-surface-and-read-model-contract.md) | task-status query contract for selector resolution, repo-plus-descendant inventory scope, stale-row visibility, and minimum text/JSON read-side result shape. |
| [019-deployment-transaction-system-contract.md](019-deployment-transaction-system-contract.md) | v0.6.0 deployment transaction contract for provider-neutral UAT and production deployment orchestration across code refs, state stacks, OCI artifacts, release evidence, provider adapters, hooks, reports, and redeploy. |
| [020-remote-bundle-sources-git-and-oci-delivery-contract.md](020-remote-bundle-sources-git-and-oci-delivery-contract.md) | unified bundle-source contract for shipped, path, git, and OCI delivery, `base_path` removal, shared source materialization, cache identity, and stale/update detection. |
| [021-docs-check-subcommand-consolidation-contract.md](021-docs-check-subcommand-consolidation-contract.md) | docs-command consolidation contract for `docs check <KIND>`, removed `check-*` spellings, and the unchanged `add-log-index` carveout. |
| [022-command-reference-completeness-and-flag-consistency-contract.md](022-command-reference-completeness-and-flag-consistency-contract.md) | bounded command-reference and repo-targeting contract for `version` documentation, missing container shapes/flags, and `--repo` widening on changelog and bundle surfaces. |
| [023-container-command-decomposition-contract.md](023-container-command-decomposition-contract.md) | structural-only module-boundary contract for splitting `src/runner/container_command/` into cache, volume, lifecycle, data, and thin-dispatch owners without widening container behavior. |
| [024-shared-dispatcher-and-exec-collapse-contract.md](024-shared-dispatcher-and-exec-collapse-contract.md) | Shared result rendering, routed container exec, and release-stage control flow without public drift. |
| [025-deploy-provider-package-contract.md](025-deploy-provider-package-contract.md) | deploy-provider package contract for moving provider-specific deployment behavior into external git/path/OCI packages backed by `provider.toml` and Rhai phase scripts. |
| [026-shared-database-target-resolution-contract.md](026-shared-database-target-resolution-contract.md) | Common database service classification and target choice for seed, dump, and state callers. |
| [027-state-domain-extraction-contract.md](027-state-domain-extraction-contract.md) | Pure state-stack model, report, path, history, and planning ownership versus runner side effects. |
| [028-manifest-section-decomposition-contract.md](028-manifest-section-decomposition-contract.md) | manifest section decomposition contract for splitting oversized manifest parsing files by durable config ownership without grammar drift. |
| [029-deploy-domain-boundary-contract.md](029-deploy-domain-boundary-contract.md) | deploy domain boundary contract for separating transaction models, report persistence, provider-package dispatch, and text rendering without schema or provider behavior drift. |
| [030-low-risk-deduplication-contract.md](030-low-risk-deduplication-contract.md) | docs-policy test ownership, CLI help topic normalization, private fixture builders, and no-behavior-change duplication cleanup rules. |
| [031-artifact-and-crate-boundary-contract.md](031-artifact-and-crate-boundary-contract.md) | artifact refs, staging, OCI, internal module ownership, small-crate retention rules, merge-candidate evidence rules, and package-map refresh triggers. |
| [032-secret-and-local-config-management-contract.md](032-secret-and-local-config-management-contract.md) | secret and local configuration contract covering config/secret separation, the built-in human-gated vault posture, secret declarations, runtime injection, consumer-repo config conventions, and Varlock adapter positioning. |
| [033-gateway-route-table-trust-contract.md](033-gateway-route-table-trust-contract.md) | gateway route-table trust boundary covering the elevated daemon's threat model, read-path integrity verification (ownership/permission + managed marker), the fail-closed behavior on an untrusted table, and operator visibility in gateway status and doctor. |
| [034-local-dependency-linking-contract.md](034-local-dependency-linking-contract.md) | machine-local dependency-linking contract covering the `effigy deps` grammar, Cargo patch and save-less Bun link mechanisms, desired state, closure, verification, drift, and lock/manifest hygiene. |
| [035-release-tag-identity-contract.md](035-release-tag-identity-contract.md) | release tag object, deterministic annotation-message, push-order, and no-retag evidence contract. |
| [037-explicit-catalog-membership-contract.md](037-explicit-catalog-membership-contract.md) | root-owned catalog membership, named and inline mounted members, shared normalization, routing stability, and ambient-discovery removal contract. |
| [038-unified-test-orchestration-contract.md](038-unified-test-orchestration-contract.md) | v0.11 single-authority test configuration, polyglot suite selection, non-executing planning, and `tasks.test` removal contract. |
| [039-pre-release-ci-proof-contract.md](039-pre-release-ci-proof-contract.md) | exact-candidate hosted CI evidence required before release preparation and execution. |
| [040-bun-committed-dependency-pinning-contract.md](040-bun-committed-dependency-pinning-contract.md) | implemented and consumer-proven root-consumer Bun overrides as a committed counterpart to machine-local links. |
| [041-documentation-graph-profile-contract.md](041-documentation-graph-profile-contract.md) | repository-owned documentation graph profiles, exact Markdown semantics, bounded context retrieval, and the Northstar runtime-independence boundary. |
| [042-external-skill-task-runner-contract.md](042-external-skill-task-runner-contract.md) | installed task-source and consumer-target separation, isolation, rejection, execution, and evidence rules. |
| [043-feature-placement-and-surface-migration-contract.md](043-feature-placement-and-surface-migration-contract.md) | semantic core placement, group-first alias-stable commands, catalog-pack simplicity and publication, release/distribution separation, and the S3 consumer gate. |
| [044-rhai-storage-create-only-contract.md](044-rhai-storage-create-only-contract.md) | Atomic create-if-absent behavior for Rhai object storage, collision handling, and redaction. |
| [045-catalog-scoped-code-graph-contract.md](045-catalog-scoped-code-graph-contract.md) | catalog-derived graph scopes, manifest posture, deterministic selection, lazy refresh, shared/independent storage, and no-sibling-work guarantees. |
| [046-published-and-draft-task-surface-contract.md](046-published-and-draft-task-surface-contract.md) | published/draft grammar, discovery isolation, lifecycle metadata, routing, reference direction, expiry evidence, and execution compatibility. |
| [047-bounded-doctor-and-scan-cache-contract.md](047-bounded-doctor-and-scan-cache-contract.md) | bounded doctor tiers, catalog scope, shared inventory, exact cache trust, deadlines, cancellation, and incomplete-report evidence. |
| [release.md](release.md) | Human-gated release sequence, verification, failure recovery, and no-retag rule. |
| [writing-style.md](writing-style.md) | Glue-light internal writing rule and reply shape. |

## Machine Contracts

- [JSON schema index](json-schema-index.json) maps command schemas to their
  validation surfaces.
- [JSON selection contract](json-selection-contract.json) defines the CI
  selection artifact.

A behavior or schema change updates its owning contract and examples in the
same PR. Historical task cards, status, and outcomes are not indexed here.
