# Roadmap g04

`g04` is the current Effigy roadmap generation.

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
- [`008-manager-backed-runtime-read-write-shell.md`](./008-manager-backed-runtime-read-write-shell.md) (active; remove old runtime compose/process command construction)
- [`009-cli-parser-modularisation-for-runtime-surfaces.md`](./009-cli-parser-modularisation-for-runtime-surfaces.md) (queued)
- [`010-drift-guards-and-architecture-proof-matrix.md`](./010-drift-guards-and-architecture-proof-matrix.md) (queued)
- [`011-contract-promotion-and-closeout.md`](./011-contract-promotion-and-closeout.md) (queued)

Architecture anchors:

- [`../../architecture/022-runtime-architecture-sanity-audit.md`](../../architecture/022-runtime-architecture-sanity-audit.md)
- [`../../architecture/010-package-map.md`](../../architecture/010-package-map.md)
- [`../../contracts/005-container-runtime-contract.md`](../../contracts/005-container-runtime-contract.md)
- [`../../contracts/009-execution-surface-convergence.md`](../../contracts/009-execution-surface-convergence.md)
- [`../../contracts/012-container-manager-contract.md`](../../contracts/012-container-manager-contract.md)
- [`../../contracts/013-task-execution-request-contract.md`](../../contracts/013-task-execution-request-contract.md)
- [`../../contracts/014-artifact-substrate-contract.md`](../../contracts/014-artifact-substrate-contract.md)

Rules:

- `g04` is the live roadmap queue
- `g03` is closed as the production export and runtime hardening generation
- no release work starts from this generation without explicit human request
- no `.github/workflows/` edits
- new runtime/container features should add request, plan, report, and adapter
  seams rather than caller-local branches
- public compatibility is preferred, but intentional cleanup breaks are allowed
  only with a roadmap card, changelog entry, guide update, and focused tests

## Next Task

Start card
[`554-extract-runtime-data-transfer-validation.md`](../../specs/batch-cards/554-extract-runtime-data-transfer-validation.md).
