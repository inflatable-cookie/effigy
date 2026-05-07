# Specs

Specs hold provisional planning surfaces for active Effigy work.

They are not a second architecture or a duplicate roadmap. Use them when a
lane needs tighter execution grammar than the roadmap alone provides.

## Working Rule

- use specs for active planning and bounded execution control
- promote durable product or behavior rules into architecture or contracts
- keep `docs/specs/` mostly limited to active or still-useful planning
- archive or remove stale specs once the durable outcome is carried elsewhere
- before roadmap generation rollover, purge stale generation-specific specs and
  batch cards from the active tree so the next generation does not inherit dead
  planning debris

Historical command-reference rule:

- active specs may preserve wrapper-script or old command references when they
  are documenting the planning state that existed at the time
- do not treat those references as current operator guidance unless the same
  command is still present in active guides/contracts
- current release/runtime/operator guidance lives in the active guides and
  contracts, not in old planning text

## Active Spec Set

- [`batch-cards/README.md`](./batch-cards/README.md)
- [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](./051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Paused but still useful:

- [`010-effigy-modularization-and-crate-boundaries-strict-lane.md`](./010-effigy-modularization-and-crate-boundaries-strict-lane.md)

Recently completed:

- [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](./050-manager-backed-runtime-read-write-shell-strict-lane.md)
- [`049-effective-container-policy-decomposition-strict-lane.md`](./049-effective-container-policy-decomposition-strict-lane.md)
- [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](./048-rhai-host-api-split-and-callback-purity-strict-lane.md)
- [`047-data-seed-dump-pipeline-strict-lane.md`](./047-data-seed-dump-pipeline-strict-lane.md)
- [`046-container-operation-pipeline-strict-lane.md`](./046-container-operation-pipeline-strict-lane.md)
- [`045-runtime-activation-pipeline-strict-lane.md`](./045-runtime-activation-pipeline-strict-lane.md)
- [`044-execution-pipeline-ownership-strict-lane.md`](./044-execution-pipeline-ownership-strict-lane.md)
- [`043-runtime-architecture-sanity-and-g04-rollover-strict-lane.md`](./043-runtime-architecture-sanity-and-g04-rollover-strict-lane.md)
- [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](./042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)
- [`041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md`](./041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md)
- [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](./040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)
- [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](./039-runtime-container-caller-migration-and-cleanup-strict-lane.md)
- [`038-plugin-ready-container-manager-facade-strict-lane.md`](./038-plugin-ready-container-manager-facade-strict-lane.md)
- [`037-canonical-task-execution-request-and-pipeline-strict-lane.md`](./037-canonical-task-execution-request-and-pipeline-strict-lane.md)
- [`036-universal-runtime-context-and-path-authority-strict-lane.md`](./036-universal-runtime-context-and-path-authority-strict-lane.md)
- [`035-v0-x-release-readiness-audit-and-gate-alignment-strict-lane.md`](./035-v0-x-release-readiness-audit-and-gate-alignment-strict-lane.md)
- [`034-next-v0-x-readiness-and-roadmap-selection-strict-lane.md`](./034-next-v0-x-readiness-and-roadmap-selection-strict-lane.md)
- [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](./033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)
- [`032-v1-runtime-hardening-proof-and-stress-matrix-strict-lane.md`](./032-v1-runtime-hardening-proof-and-stress-matrix-strict-lane.md)
- [`031-architecture-map-and-authority-surface-repair-strict-lane.md`](./031-architecture-map-and-authority-surface-repair-strict-lane.md)
- [`030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md`](./030-container-and-runtime-error-taxonomy-and-diagnostics-strict-lane.md)
- [`029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`](./029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md)
- [`028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`](./028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md)
- [`027-runtime-session-context-and-runtime-ownership-hardening-strict-lane.md`](./027-runtime-session-context-and-runtime-ownership-hardening-strict-lane.md)
- [`026-decodelabs-production-strategy-scope-strict-lane.md`](./026-decodelabs-production-strategy-scope-strict-lane.md)
- [`025-regression-matrix-and-drift-guards-strict-lane.md`](./025-regression-matrix-and-drift-guards-strict-lane.md)
- [`024-embedded-command-script-and-bootstrap-convergence-strict-lane.md`](./024-embedded-command-script-and-bootstrap-convergence-strict-lane.md)
- [`023-interactive-session-ownership-and-lifecycle-convergence-strict-lane.md`](./023-interactive-session-ownership-and-lifecycle-convergence-strict-lane.md)
- [`022-execution-surface-convergence-strict-lane.md`](./022-execution-surface-convergence-strict-lane.md)
- [`001-production-deployment-model-and-export-contract-strict-lane.md`](./001-production-deployment-model-and-export-contract-strict-lane.md)
- [`021-unified-init-and-starter-emission-strict-lane.md`](./021-unified-init-and-starter-emission-strict-lane.md)
- [`013-dev-front-door-and-managed-lifecycle-strict-lane.md`](./013-dev-front-door-and-managed-lifecycle-strict-lane.md)
- [`015-persistent-data-and-volume-lifecycle-strict-lane.md`](./015-persistent-data-and-volume-lifecycle-strict-lane.md)
- [`016-multi-project-coordination-strict-lane.md`](./016-multi-project-coordination-strict-lane.md)
- [`014-rust-native-gateway-strict-lane.md`](./014-rust-native-gateway-strict-lane.md)
- [`012-container-context-and-transparent-execution-strict-lane.md`](./012-container-context-and-transparent-execution-strict-lane.md)
- [`011-service-catalog-and-compose-assembly-strict-lane.md`](./011-service-catalog-and-compose-assembly-strict-lane.md)
- [`020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`](./020-multi-project-gateway-expansion-and-service-dns-strict-lane.md)
- [`007-distribution-release-and-consumer-rollout-strict-lane.md`](./007-distribution-release-and-consumer-rollout-strict-lane.md)

## Next Task

Card
[`567-extract-release-parser-module.md`](./batch-cards/567-extract-release-parser-module.md).
