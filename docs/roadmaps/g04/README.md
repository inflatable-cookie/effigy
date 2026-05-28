# Roadmap g04

`g04` is the completed Effigy runtime architecture simplification generation.

Generation theme:

- make Effigy's runtime/container core boring, typed, and inspectable
- replace caller-local orchestration with explicit pipelines
- prioritize ownership purity over cosmetic file splitting
- keep public behavior stable unless a cleanup break is deliberately selected
- prove runtime/container paths through focused plan and dispatch tests

Current milestones:

- [`001-runtime-architecture-sanity-audit-and-generation-rollover.md`](./001-runtime-architecture-sanity-audit-and-generation-rollover.md) (complete; audit landed and g04 opened)
- [`002-execution-pipeline-ownership.md`](./002-execution-pipeline-ownership.md) (complete; shared execution planning surfaces landed)
- [`003-runtime-activation-pipeline.md`](./003-runtime-activation-pipeline.md) (complete; runtime activation plan and stage path landed)
- [`004-container-operation-pipeline.md`](./004-container-operation-pipeline.md) (complete; container operations moved behind typed plans and manager invocation surfaces)
- [`005-data-seed-dump-pipeline.md`](./005-data-seed-dump-pipeline.md) (complete; data seed/dump decisions moved behind `effigy-data`)
- [`006-rhai-host-api-split-and-callback-purity.md`](./006-rhai-host-api-split-and-callback-purity.md) (complete; Rhai host API split and callback purity landed)
- [`007-effective-container-policy-decomposition.md`](./007-effective-container-policy-decomposition.md) (complete; split `effigy-containers` policy, workspace, generated compose, and exec ownership)
- [`008-manager-backed-runtime-read-write-shell.md`](./008-manager-backed-runtime-read-write-shell.md) (complete; runtime data/read/write/shell modules split behind manager-plan seams)
- [`009-cli-parser-modularisation-for-runtime-surfaces.md`](./009-cli-parser-modularisation-for-runtime-surfaces.md) (complete; high-churn parser surfaces split below target sizes)
- [`010-drift-guards-and-architecture-proof-matrix.md`](./010-drift-guards-and-architecture-proof-matrix.md) (complete; drift guard and proof matrix landed)
- [`011-contract-promotion-and-closeout.md`](./011-contract-promotion-and-closeout.md) (complete; promoted the current g04 architecture set into contracts)
- [`012-runtime-pipeline-integration-audit-and-debt-map.md`](./012-runtime-pipeline-integration-audit-and-debt-map.md) (complete; integration debt mapped and next implementation order selected)
- [`013-runtime-activation-route-and-plan-authority.md`](./013-runtime-activation-route-and-plan-authority.md) (complete; activation route identity and shared builder landed)
- [`014-data-seed-dump-plan-consumption.md`](./014-data-seed-dump-plan-consumption.md) (complete; `effigy-data` plans now drive seed/dump flows)
- [`015-container-volume-operation-pipeline.md`](./015-container-volume-operation-pipeline.md) (complete; volume inventory has a typed operation plan)
- [`016-architecture-guard-integration.md`](./016-architecture-guard-integration.md) (complete; architecture guards are wired into normal validation)
- [`017-planning-crate-decomposition.md`](./017-planning-crate-decomposition.md) (complete; first planning-crate decomposition pass landed)
- [`018-oci-artifact-closeout-and-proof-matrix.md`](./018-oci-artifact-closeout-and-proof-matrix.md) (complete; OCI support now has proof, remediation, and contract closeout)
- [`019-state-stack-and-layered-seed-framework.md`](./019-state-stack-and-layered-seed-framework.md) (complete; state-stack framework, apply/capture/history surfaces, and Example App proof loop landed for the current release boundary)
- [`020-task-status-record-and-active-run-model.md`](./020-task-status-record-and-active-run-model.md) (complete; canonical task-status identity, active/completed persistence, direct-path write-side ownership, and stale reconciliation helpers landed)
- [`021-task-status-query-surface-and-read-model.md`](./021-task-status-query-surface-and-read-model.md) (complete; `effigy tasks status <selector>` and `--all` are both landed on the shared task-status record model)
- [`022-remote-bundle-sources-git-and-oci-delivery.md`](./022-remote-bundle-sources-git-and-oci-delivery.md) (complete; unified `[bundle].base` typed source forms, removed `base_path`, and landed git/OCI bundle resolution, sync, and inspect)
- [`023-docs-check-subcommand-consolidation.md`](./023-docs-check-subcommand-consolidation.md) (complete; collapsed the flat `docs check-*` surface into `docs check <KIND>` and removed the old spellings with migration errors)
- [`024-command-reference-completeness-and-flag-consistency.md`](./024-command-reference-completeness-and-flag-consistency.md) (complete; command matrix gaps are closed and repo-local `changelog`/`bundle` surfaces now accept bounded `--repo` targeting)
- [`025-container-command-decomposition.md`](./025-container-command-decomposition.md) (complete; split `container_command/` into cache, volume, lifecycle, data, and thin shared dispatch owners without behavior drift)
- [`026-shared-dispatcher-and-exec-collapse.md`](./026-shared-dispatcher-and-exec-collapse.md) (complete; landed the shared render helper, collapsed routed container-exec duplication, and shared the release stage control flow)
- [`027-deployment-transaction-system.md`](./027-deployment-transaction-system.md) (complete; defined the v0.6.0 deployment transaction contract and provider-neutral execution posture)
- [`028-deployment-config-plan-and-reporting.md`](./028-deployment-config-plan-and-reporting.md) (complete; added deploy env config, `deploy plan <env>`, and durable plan reports)
- [`029-railway-deployment-adapter.md`](./029-railway-deployment-adapter.md) (complete; added Railway deployment transaction support through the provider report boundary)
- [`030-render-deployment-adapter.md`](./030-render-deployment-adapter.md) (complete; added Render support through the same provider-neutral deployment transaction model)
- [`031-deployment-status-history-and-redeploy.md`](./031-deployment-status-history-and-redeploy.md) (complete; added deployment status, history, and evidence-backed redeploy)
- [`032-example-app-deployment-proof-and-closeout.md`](./032-example-app-deployment-proof-and-closeout.md) (complete; documented the Example App UAT/production deployment loop and closed the suite)
- [`033-post-release-reference-grade-cleanup-suite.md`](./033-post-release-reference-grade-cleanup-suite.md) (complete; opened the post-v0.6.x audit cleanup suite)
- [`034-shared-database-target-resolution.md`](./034-shared-database-target-resolution.md) (complete; converged seed/dump database target selection behind a shared resolver)
- [`035-state-domain-extraction.md`](./035-state-domain-extraction.md) (complete; moved state report paths, history, apply planning, and capture planning into `effigy-state`)
- [`036-manifest-section-decomposition.md`](./036-manifest-section-decomposition.md) (complete; split bundle source/cache and manifest config sections into bounded owners)
- [`037-deploy-domain-boundary-hardening.md`](./037-deploy-domain-boundary-hardening.md) (complete; separated deploy transaction reports, provider context, and text rendering)
- [`038-docs-policy-cli-help-and-test-fixture-deduplication.md`](./038-docs-policy-cli-help-and-test-fixture-deduplication.md) (complete; removed high-confidence docs-policy duplication and centralized safe help/fixture pieces)
- [`039-artifact-and-crate-boundary-rejustification.md`](./039-artifact-and-crate-boundary-rejustification.md) (complete; split artifact internals and refreshed crate-boundary posture)

Architecture anchors:

- [`../../architecture/022-runtime-architecture-sanity-audit.md`](../../architecture/022-runtime-architecture-sanity-audit.md)
- [`../../architecture/010-package-map.md`](../../architecture/010-package-map.md)
- [`../../contracts/005-container-runtime-contract.md`](../../contracts/005-container-runtime-contract.md)
- [`../../contracts/009-execution-surface-convergence.md`](../../contracts/009-execution-surface-convergence.md)
- [`../../contracts/012-container-manager-contract.md`](../../contracts/012-container-manager-contract.md)
- [`../../contracts/013-task-execution-request-contract.md`](../../contracts/013-task-execution-request-contract.md)
- [`../../contracts/014-artifact-substrate-contract.md`](../../contracts/014-artifact-substrate-contract.md)
- [`../../contracts/017-task-status-record-and-active-run-model-contract.md`](../../contracts/017-task-status-record-and-active-run-model-contract.md)
- [`../../contracts/019-deployment-transaction-system-contract.md`](../../contracts/019-deployment-transaction-system-contract.md)

Rules:

- `g04` is closed as the runtime architecture simplification generation
- `g03` is closed as the production export and runtime hardening generation
- no release work starts from this generation without explicit human request
- no `.github/workflows/` edits
- new runtime/container features should add request, plan, report, and adapter
  seams rather than caller-local branches
- public compatibility is preferred, but intentional cleanup breaks are allowed
  only with a roadmap card, changelog entry, guide update, and focused tests
- queued follow-on lanes may be opened while one strict lane remains active,
  but do not open a second active strict lane until the current one closes or
  is deliberately paused

## Next Task

Plan the `g05` generation theme and rollover cleanup before opening new
implementation roadmaps. Release execution remains human-owned.


Batch cards live in `g04/batch-cards/` when strict posture uses them.
