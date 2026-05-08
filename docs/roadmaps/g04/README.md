# Roadmap g04

`g04` is the current Effigy runtime architecture simplification generation.

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

Architecture anchors:

- [`../../architecture/022-runtime-architecture-sanity-audit.md`](../../architecture/022-runtime-architecture-sanity-audit.md)
- [`../../architecture/010-package-map.md`](../../architecture/010-package-map.md)
- [`../../contracts/005-container-runtime-contract.md`](../../contracts/005-container-runtime-contract.md)
- [`../../contracts/009-execution-surface-convergence.md`](../../contracts/009-execution-surface-convergence.md)
- [`../../contracts/012-container-manager-contract.md`](../../contracts/012-container-manager-contract.md)
- [`../../contracts/013-task-execution-request-contract.md`](../../contracts/013-task-execution-request-contract.md)
- [`../../contracts/014-artifact-substrate-contract.md`](../../contracts/014-artifact-substrate-contract.md)

Rules:

- `g04` remains the current runtime architecture simplification generation
- `g03` is closed as the production export and runtime hardening generation
- no release work starts from this generation without explicit human request
- no `.github/workflows/` edits
- new runtime/container features should add request, plan, report, and adapter
  seams rather than caller-local branches
- public compatibility is preferred, but intentional cleanup breaks are allowed
  only with a roadmap card, changelog entry, guide update, and focused tests

## Next Task

No ready card. Planning stop.


Batch cards live in `g04/batch-cards/` when strict posture uses them.
