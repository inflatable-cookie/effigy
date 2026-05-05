# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning
- keep the active tree focused on live or near-live cards; archive stale cards
  once their lane is closed or paused cleanly
- do not use this index as a graveyard dump of every historical card

## Current Live Chain

- [`382-scaffold-container-manager-contract-and-crate.md`](./382-scaffold-container-manager-contract-and-crate.md)
  is complete. It opened `g03.031` and created the first
  `effigy-container-manager` facade slice.

- [`383-move-compose-backend-detection-behind-container-manager.md`](./383-move-compose-backend-detection-behind-container-manager.md)
  is complete. It moved compose backend detection and invocation shape behind
  the manager facade.

- [`384-migrate-container-lifecycle-through-manager.md`](./384-migrate-container-lifecycle-through-manager.md)
  is complete. It migrated lifecycle commands through the manager facade.

- [`385-migrate-exec-copy-data-through-manager.md`](./385-migrate-exec-copy-data-through-manager.md)
  is complete. It migrated exec, copy, and data operation branching through the
  manager facade.

- [`386-close-container-manager-facade-lane.md`](./386-close-container-manager-facade-lane.md)
  is ready. It closes `g03.031` with drift guards and contract/readme
  alignment.

- [`381-migrate-embedded-task-dispatch-to-execution-request.md`](./381-migrate-embedded-task-dispatch-to-execution-request.md)
  is complete. It moved embedded task dispatch onto `TaskExecutionRequestBuilder`.

- [`380-migrate-direct-task-dispatch-to-execution-request.md`](./380-migrate-direct-task-dispatch-to-execution-request.md)
  is complete. It moved direct task dispatch onto `TaskExecutionRequestBuilder`.

- [`379-expose-rhai-exec-run-through-execution-request.md`](./379-expose-rhai-exec-run-through-execution-request.md)
  is complete. It exposes Rhai `exec::run(...)` through the execution request
  builder.

- [`378-scaffold-canonical-execution-request-crate.md`](./378-scaffold-canonical-execution-request-crate.md)
  is complete. It opened the first `effigy-execution` crate slice.

- [`377-expose-rhai-runtime-context-helper.md`](./377-expose-rhai-runtime-context-helper.md)
  is complete. It exposes the captured runtime context to Rhai before the
  execution helper lands.

- [`376-design-rhai-runtime-context-and-execution-helper.md`](./376-design-rhai-runtime-context-and-execution-helper.md)
  is complete. It turned the DecodeLabs mysql seed path bug into a concrete Rhai
  runtime-context and execution-builder requirement.

- [`375-migrate-command-local-cwd-root-callers.md`](./375-migrate-command-local-cwd-root-callers.md)
  is complete. It moved direct runner cwd/root helpers behind the active
  `EffigyRuntimeContext` and inventoried the Rhai follow-up.

- [`374-plan-runtime-context-contract-and-crate-boundary.md`](./374-plan-runtime-context-contract-and-crate-boundary.md)
  is complete. It opened the runtime context lane and landed the first
  `effigy-context` slice.

- [`373-audit-v0-x-release-readiness-and-gate-alignment.md`](./373-audit-v0-x-release-readiness-and-gate-alignment.md)
  is complete. It audited current `v0.x` release readiness without release
  execution.

- [`372-decide-next-live-roadmap-after-prompt-lane-closeout.md`](./372-decide-next-live-roadmap-after-prompt-lane-closeout.md)
  is complete. It selected `g03.029` as the next live roadmap.

- [`371-close-interactive-cli-prompt-expansion-lane.md`](./371-close-interactive-cli-prompt-expansion-lane.md)
  is complete. `g03.027` is closed.

- [`370-decide-post-broad-unlock-confirmation-boundary.md`](./370-decide-post-broad-unlock-confirmation-boundary.md)
  is complete. Optional `init` starter selection is out of scope for this
  guardrail lane.

- [`369-implement-broad-unlock-confirmation.md`](./369-implement-broad-unlock-confirmation.md)
  is complete. Broad `unlock` actions now require confirmation in eligible
  interactive flows and use `--yes` as the automation bypass.

- [`368-promote-prompt-policy-for-builtin-unlock.md`](./368-promote-prompt-policy-for-builtin-unlock.md)
  is complete. The shared prompt policy now lives in `effigy-builtin`, where
  runner and built-in prompt surfaces can both use it.

- [`367-decide-post-container-data-import-confirmation-boundary.md`](./367-decide-post-container-data-import-confirmation-boundary.md)
  is complete. Broad `unlock` confirmation needs a prompt-policy promotion
  prerequisite because `unlock` lives in `effigy-builtin`.

- [`366-implement-container-data-import-confirmation.md`](./366-implement-container-data-import-confirmation.md)
  is complete. `container data import` now requires confirmation in eligible
  interactive flows and uses `--yes` as the automation bypass.

- [`365-decide-post-container-data-pull-production-confirmation-boundary.md`](./365-decide-post-container-data-pull-production-confirmation-boundary.md)
  is complete. The next prompt seam is `container data import` because it can
  overwrite local generated-compose data and is part of the lane exit
  condition.

- [`364-implement-container-data-pull-production-confirmation.md`](./364-implement-container-data-pull-production-confirmation.md)
  is complete. `container data pull-production` now requires confirmation in
  eligible interactive flows and uses `--yes` as the automation bypass.

- [`363-decide-post-bootstrap-path-reuse-prompt-boundary.md`](./363-decide-post-bootstrap-path-reuse-prompt-boundary.md)
  is complete. The policy is strong enough to widen directly into
  `container data pull-production`, with `--yes` as the explicit automation
  bypass.

- [`362-implement-prompt-policy-and-bootstrap-path-reuse-confirmation.md`](./362-implement-prompt-policy-and-bootstrap-path-reuse-confirmation.md)
  is complete. Bootstrap now has a shared prompt policy and confirms reuse of
  existing non-empty destinations only in eligible interactive flows.

- [`361-decide-post-host-integration-proof-boundary.md`](./361-decide-post-host-integration-proof-boundary.md)
  is complete. `g03.018` now closes cleanly because the remaining
  host-integration and shared-service seams are proven strongly enough to stop
  treating the runtime/container core as under-hardened for `v1.0`.

- [`360-implement-host-integration-and-shared-service-proof-slice.md`](./360-implement-host-integration-and-shared-service-proof-slice.md)
  is complete. The runtime/container proof lane now includes one integrated
  stack proof for host Composer home, explicit SSH-home mounting, external
  mounts, and shared-service env projection.

- [`359-decide-post-proof-matrix-foundation-boundary.md`](./359-decide-post-proof-matrix-foundation-boundary.md)
  is complete. The lane stays open because `358` proved the bootstrap/lease/
  workspace ownership seams, but not the remaining host-integration and
  shared-service seams.

- [`358-implement-runtime-container-proof-matrix-foundation.md`](./358-implement-runtime-container-proof-matrix-foundation.md)
  is complete. The first bounded runtime/container proof matrix now exists
  across bootstrap runtime-session posture, reused-runtime lease parity, and
  direct-versus-seeded workspace cleanup behavior.

- [`357-decide-post-architecture-authority-foundation-boundary.md`](./357-decide-post-architecture-authority-foundation-boundary.md)
  is complete. `g03.017` now closes cleanly because the runtime/container
  authority problem was the stale package/ownership surface, not a need for
  endless architecture churn.

- [`356-inventory-and-repair-runtime-container-authority-surfaces.md`](./356-inventory-and-repair-runtime-container-authority-surfaces.md)
  is complete. The live package map now reflects the post-hardening code
  seams, the architecture overview points at the right authority surfaces,
  and the old container design doc is explicitly background reference rather
  than live ownership truth.

- [`355-decide-post-gateway-final-error-boundary.md`](./355-decide-post-gateway-final-error-boundary.md)
  is complete. `g03.016` now closes cleanly because the runtime/container core
  no longer relies on string-first translation as the dominant failure shape.

- [`354-implement-typed-gateway-runtime-row-and-port-binding-translation-errors.md`](./354-implement-typed-gateway-runtime-row-and-port-binding-translation-errors.md)
  is complete. Gateway reconciliation now also covers typed runtime-row
  discovery, service-alias lookup, and raw port-binding translation.

- [`353-decide-post-gateway-closeout-error-boundary.md`](./353-decide-post-gateway-closeout-error-boundary.md)
  is complete. `g03.016` stays open because `352` landed the loopback and
  runtime-target slice, but top-level runtime-row discovery plus raw
  port-binding/service-alias translation are still string-first.

- [`352-implement-typed-gateway-loopback-and-runtime-target-translation-errors.md`](./352-implement-typed-gateway-loopback-and-runtime-target-translation-errors.md)
  is complete. Gateway reconciliation now uses typed `RunnerError` families
  for loopback registry load/save/allocation, runtime-target validation, and
  remaining route-target selection seams instead of flattening those failures
  into generic invocation strings.

- [`351-decide-post-gateway-reconciliation-error-boundary.md`](./351-decide-post-gateway-reconciliation-error-boundary.md)
  is complete. `g03.016` stays open because `350` proved the gateway taxonomy
  path is real, but `gateway_registration.rs` still has too many generic
  invocation translations for loopback allocation and runtime-target checks to
  call the lane done.

- [`350-implement-typed-gateway-reconciliation-and-route-translation-errors.md`](./350-implement-typed-gateway-reconciliation-and-route-translation-errors.md)
  is complete. Gateway reconciliation now uses typed `RunnerError` families
  for route-table load/save, route register/deregister, and the first
  route-shape validation seams instead of flattening those failures into
  generic invocation strings.

- [`349-decide-post-workspace-handoff-and-lease-error-boundary.md`](./349-decide-post-workspace-handoff-and-lease-error-boundary.md)
  is complete. `g03.016` stays open because `348` fixed the workspace-session
  and lease seam, but gateway and route reconciliation still keep generic
  invocation strings as the dominant shape in the remaining runtime/container
  path.

- [`348-implement-typed-workspace-handoff-and-lease-error-translation.md`](./348-implement-typed-workspace-handoff-and-lease-error-translation.md)
  is complete. Public workspace shell handoff plus host-container lease encode
  and reaper bootstrap errors now use typed `RunnerError` families instead of
  flattening those session and lease failures into generic invocation strings.

- [`347-decide-post-typed-container-surface-and-policy-boundary.md`](./347-decide-post-typed-container-surface-and-policy-boundary.md)
  is complete. `g03.016` stays open because `346` fixed the exec-surface path,
  but workspace handoff and lease translation still keep generic invocation
  strings as the dominant shape in the remaining runtime/session failure seam.

- [`346-implement-typed-container-surface-and-policy-translation-errors.md`](./346-implement-typed-container-surface-and-policy-translation-errors.md)
  is complete. `effigy exec` container-surface resolution and one
  policy-translation seam now use typed `RunnerError` families instead of
  flattening those failures into generic invocation strings.

- [`345-decide-post-typed-runtime-container-error-foundation-boundary.md`](./345-decide-post-typed-runtime-container-error-foundation-boundary.md)
  is complete. The lane stays open because `344` proved the error-taxonomy path
  is real, but did not yet stop generic invocation strings from dominating the
  container surface and policy-translation seams.

- [`344-implement-typed-runtime-container-error-foundation.md`](./344-implement-typed-runtime-container-error-foundation.md)
  is complete. Container runtime prep now uses typed error families for
  policy-validation and exec-readiness failures instead of flattening those
  seams into generic invocation strings.

- [`343-decide-post-workspace-provisioning-split-boundary.md`](./343-decide-post-workspace-provisioning-split-boundary.md)
  is complete. `g03.015` now closes cleanly because `workspace.rs` is no longer
  carrying the same mixed ownership risk after the session and provisioning
  splits.

- [`342-implement-workspace-provisioning-split-foundation.md`](./342-implement-workspace-provisioning-split-foundation.md)
  is complete. Workspace artifact/binary provisioning plus permission prep now
  sit under one dedicated `workspace_provisioning` owner instead of caller-local
  sequencing in `workspace.rs`.

- [`340-implement-workspace-session-orchestrator-foundation.md`](./340-implement-workspace-session-orchestrator-foundation.md)
  is complete. Public workspace entry and bootstrap start handoff now share
  one explicit workspace-session owner instead of keeping the whole lifecycle
  inline in `workspace.rs`.

- [`341-decide-post-workspace-session-orchestrator-foundation-boundary.md`](./341-decide-post-workspace-session-orchestrator-foundation-boundary.md)
  is complete. `g03.015` stays open because workspace provisioning plus
  permission/env preparation still sit in the same hotspot.

- [`336-implement-typed-container-assembly-foundation.md`](./336-implement-typed-container-assembly-foundation.md)
  is complete. `effigy-containers` now has a first typed generated-compose
  document, and shared-service env injection plus generated port publication
  no longer each reparse compose YAML as their working data model.

- [`337-decide-post-typed-container-assembly-foundation-boundary.md`](./337-decide-post-typed-container-assembly-foundation-boundary.md)
  is complete. `g03.014` stays open because generated media and host mount
  attachment still reparse YAML and rediscover repo-root-attached services
  ad hoc inside `policy_support.rs`.

- [`338-implement-typed-mount-attachment-assembly-slice.md`](./338-implement-typed-mount-attachment-assembly-slice.md)
  is complete. Generated media and host mount attachment now sit on the typed
  generated-compose model, and repo-root-attached service detection no longer
  rediscovers that seam from raw YAML.

- [`339-decide-post-typed-mount-attachment-boundary.md`](./339-decide-post-typed-mount-attachment-boundary.md)
  is complete. `g03.014` now closes cleanly, and the remaining rewrite-heavy
  work moves to the workspace/runtime orchestrator split lane.

- [`334-implement-typed-activation-and-session-context-foundation.md`](./334-implement-typed-activation-and-session-context-foundation.md)
  is complete. Bootstrap setup work, workspace handoff, seeded shells, routed
  activation, deferred container activation, and `effigy exec` now share one
  typed runtime/session context for lease policy and bootstrap stop-on-exit
  ownership instead of ambient bootstrap-only env flags.

- [`335-decide-post-typed-activation-and-session-context-foundation-boundary.md`](./335-decide-post-typed-activation-and-session-context-foundation-boundary.md)
  is complete. `g03.013` now closes cleanly, and the next honest hardening
  seam is the YAML-rewrite-heavy container assembly core rather than another
  ownership follow-up slice.

- [`333-decide-post-decodelabs-inventory-boundary.md`](./333-decide-post-decodelabs-inventory-boundary.md)
  is complete. `g03.003` now closes on an explicit short-term answer:
  Decodelabs production remains operator-owned until a real converged topology
  or promotion trigger exists.

- [`332-inventory-decodelabs-production-deployment-shape.md`](./332-inventory-decodelabs-production-deployment-shape.md)
  is complete. The first Decodelabs production inventory is now captured in
  the contract and roadmap surfaces strongly enough to drive a real boundary
  decision.

- [`331-implement-runtime-side-effect-parity-closeout.md`](./331-implement-runtime-side-effect-parity-closeout.md)
  is complete. The remaining runtime side-effect parity gaps are now covered:
  bootstrap start plus workspace-handoff anchors, shared runtime-prep ordering
  for gateway/alias and lease side effects, and the final lease/gateway
  parity proof on the shared activation owner.

- [`330-decide-post-parity-matrix-foundation-boundary.md`](./330-decide-post-parity-matrix-foundation-boundary.md)
  is complete. The honest next move after `329` is not lane closeout yet; one
  more bounded runtime side-effect parity slice is still warranted.

- [`329-implement-convergence-parity-matrix-foundation.md`](./329-implement-convergence-parity-matrix-foundation.md)
  is complete. The first bounded parity matrix now exists across shared
  embedded repo targeting, Rhai/default repo targeting, explicit exec
  activation, deferred container activation, direct versus seeded interactive
  ownership, and the intentional inline-workspace unsupported-surface family.

- [`328-decide-post-embedded-runner-foundation-boundary.md`](./328-decide-post-embedded-runner-foundation-boundary.md)
  is complete. Bootstrap managed-run synthesis is not a normal embedded
  replay surface, so `g03.011` does not widen again before handing off to
  the drift-guard lane.

- [`327-implement-shared-embedded-runner-foundation.md`](./327-implement-shared-embedded-runner-foundation.md)
  is complete. Rhai command replay, run-array builtins, and bootstrap task
  dispatch now share the first embedded-runner spine.

- [`326-decide-post-interactive-ownership-foundation-boundary.md`](./326-decide-post-interactive-ownership-foundation-boundary.md)
  is complete. The next honest widening seam after `325` is not attached
  operator sessions; that surface remains an explicit operator lifecycle
  exception, so the convergence program hands off to `g03.011`.

- [`325-implement-interactive-ownership-classification-foundation.md`](./325-implement-interactive-ownership-classification-foundation.md)
  is complete. Direct `effigy workspace` entry and seeded task shells now
  derive adopted-versus-session-owned cleanup from one shared ownership helper
  instead of caller-local booleans.

- [`324-implement-exec-activation-convergence-foundation.md`](./324-implement-exec-activation-convergence-foundation.md)
  is complete. `effigy exec`, exec aliases, and named-container/default
  dev-container exec now share the bounded non-shell activation contract.

- [`312-decide-post-deploy-model-foundation-widening.md`](./312-decide-post-deploy-model-foundation-widening.md)
  is complete. The next `g03.001` move is now explicit: one more
  neutral-model strengthening batch before any provider adapter work starts.

- [`313-strengthen-deploy-model-production-metadata-foundation.md`](./313-strengthen-deploy-model-production-metadata-foundation.md)
  is complete. `deploy.model.v1` now carries the first honest production
  metadata seams provider adapters need: static output ownership, shared API
  health promotion, and `db:migrate` release-hook promotion when present.

- [`314-decide-post-production-metadata-widening.md`](./314-decide-post-production-metadata-widening.md)
  is complete. The next widening seam is now explicit: provider-export
  planning before adapter implementation, with Render first.

- [`315-plan-first-render-export-contract.md`](./315-plan-first-render-export-contract.md)
  is complete. The first Render adapter boundary is now explicit, and it also
  exposed the remaining neutral-model gap around static fallback ownership.

- [`316-strengthen-static-fallback-ownership-for-render-export.md`](./316-strengthen-static-fallback-ownership-for-render-export.md)
  is complete. The neutral model now carries static fallback ownership, so the
  Render contract no longer has to block on SPA rewrite ambiguity.

- [`317-implement-render-export-foundation.md`](./317-implement-render-export-foundation.md)
  is complete. The first bounded Render export path now exists through
  `deploy export render`, including `render.yaml` generation, plan mode, and
  model-to-blueprint mapping for the shipped Underlay shape.

- [`318-decide-post-render-export-foundation-boundary.md`](./318-decide-post-render-export-foundation-boundary.md)
  is complete. The post-Render boundary is now explicit: one real Underlay
  proof comes before Railway planning.

- [`319-prove-render-export-in-one-real-underlay-repo.md`](./319-prove-render-export-in-one-real-underlay-repo.md)
  is complete. The first Render exporter now has one real Underlay proof in
  `underlay-reference`, and that proof did not expose exporter drift.

- [`320-decide-post-render-proof-provider-widening.md`](./320-decide-post-render-proof-provider-widening.md)
  is complete. The next honest widening seam is now explicit: Railway planning
  instead of more Render churn.

- [`321-plan-first-railway-export-contract.md`](./321-plan-first-railway-export-contract.md)
  is complete. The first bounded Railway adapter contract now exists, including
  service-local config ownership and report-owned operator follow-up.

- [`322-implement-railway-export-foundation.md`](./322-implement-railway-export-foundation.md)
  is complete. The first bounded Railway export path now exists, including
  service-local config generation plus report-owned operator follow-up.

- [`323-decide-post-railway-export-foundation-boundary.md`](./323-decide-post-railway-export-foundation-boundary.md)
  is complete. `g03.001` is now closed; Render and Railway both have bounded
  Underlay-first foundations, and there is no active ready card.

- [`304-decide-post-release-closure-v0-3-prep-follow-up.md`](./304-decide-post-release-closure-v0-3-prep-follow-up.md)
  is complete. The next `g02.007` move is now explicit again: one bounded
  `v0.3.0` release-prep alignment slice before any human-approved release
  action.
- [`305-implement-v0-3-release-prep-alignment.md`](./305-implement-v0-3-release-prep-alignment.md)
  is complete. The release-prep checkpoint is now refreshed from live command
  evidence, and the lane is back in planning pending explicit human-approved
  release execution for `v0.3.0`.

- [`260-decide-post-modularization-integration-spine-for-g02-011-through-g02-016.md`](./260-decide-post-modularization-integration-spine-for-g02-011-through-g02-016.md)
  is complete. The post-`g02.010` integration order is now explicit:
  `011` first, then `012`, then `014`, then `015`, then `016`, with
  `013` treated as the downstream aggregator milestone.
- [`261-implement-service-catalog-integration-foundation.md`](./261-implement-service-catalog-integration-foundation.md)
  is complete. The first real product integration batch is now landed:
  manifest service declarations, generated compose ownership in
  `effigy-containers`, and schema acceptance all now exist on the product
  path.
- [`262-implement-catalog-and-eject-product-surface.md`](./262-implement-catalog-and-eject-product-surface.md)
  is complete. The visible `g02.011` operator surface is now real:
  `catalog list`, `catalog extract`, and `container eject`.
- [`263-prove-service-catalog-loop-in-one-real-project.md`](./263-prove-service-catalog-loop-in-one-real-project.md)
  is complete. The full generated-compose loop is now proven in
  `underlay-reference`, including an in-batch fix for manifest rewrite during
  `container eject`.
- [`264-implement-context-routing-foundation.md`](./264-implement-context-routing-foundation.md)
  is complete. The first bounded `g02.012` integration slice is now landed:
  manifest context support plus routing integration through normal task
  dispatch.
- [`265-implement-explicit-exec-and-alias-surface.md`](./265-implement-explicit-exec-and-alias-surface.md)
  is complete. The visible `g02.012` exec surface is now landed: explicit
  exec, aliases, CWD mapping, handoff behavior, and a real consumer proof.
- [`266-implement-gateway-command-foundation.md`](./266-implement-gateway-command-foundation.md)
  is complete. The host-native `gateway up/down/status` surface, detached
  daemon path, and startup diagnostics are now landed.
- [`267-implement-gateway-route-registration-foundation.md`](./267-implement-gateway-route-registration-foundation.md)
  is complete. Manifest DNS declaration and container lifecycle route
  registration are now wired into the product path.
- [`268-prove-plain-http-gateway-hostname-loop-in-one-real-project.md`](./268-prove-plain-http-gateway-hostname-loop-in-one-real-project.md)
  is complete. The plain HTTP hostname loop is now proven on one real project,
  and the proof hardened gateway registration with
  `[containers.<name>.dns].port` for multi-port stacks.
- [`269-plan-gateway-tls-closeout-batch.md`](./269-plan-gateway-tls-closeout-batch.md)
  is complete. The remaining gateway TLS work is now bounded on a trustworthy
  product boundary instead of left implicit.
- [`270-implement-gateway-tls-closeout.md`](./270-implement-gateway-tls-closeout.md)
  is complete. The gateway now has a real TLS product path, including
  `setup-tls`, route-owned cert lifecycle, honest readiness/status projection,
  and one real HTTPS consumer proof.
- [`271-plan-multi-project-coordination-status-batch.md`](./271-plan-multi-project-coordination-status-batch.md)
  is complete. The broad `g02.016` coordination roadmap now has one bounded
  first execution target instead of a vague handoff from gateway closeout.
- [`272-implement-cross-project-status-and-route-dashboard-foundation.md`](./272-implement-cross-project-status-and-route-dashboard-foundation.md)
  is complete. The first real `g02.016` coordination surface is now landed:
  `container status --all` plus a fuller shared route dashboard in
  `gateway status`.
- [`273-plan-port-auto-allocation-batch.md`](./273-plan-port-auto-allocation-batch.md)
  is complete. The next `g02.016` follow-up is now explicit: generated-compose
  port auto-allocation before any stats or shared-service widening.
- [`274-implement-generated-compose-port-auto-allocation.md`](./274-implement-generated-compose-port-auto-allocation.md)
  is complete. Generated compose now owns effective host-port publication on
  the product path: explicit manifest `host.ports` is wired through generated
  compose, and omitted `host.ports` now allocate stable ports through the
  shared registry.
- [`275-plan-resource-stats-batch.md`](./275-plan-resource-stats-batch.md)
  is complete. The next `g02.016` follow-up is now explicit again:
  cross-project resource stats before any shared-service widening.
- [`276-implement-container-resource-stats-foundation.md`](./276-implement-container-resource-stats-foundation.md)
  is complete. The container surface now has one bounded cross-project
  resource view through `container stats --all`, including honest warnings
  when runtime stats are partial or unavailable.
- [`277-plan-shared-services-closeout-batch.md`](./277-plan-shared-services-closeout-batch.md)
  is complete. The final `g02.016` move is now explicit again: bounded
  generated-compose shared services instead of a vague shared-service promise.
- [`278-implement-generated-compose-shared-services.md`](./278-implement-generated-compose-shared-services.md)
  is complete. `g02.016` is now closed on the shipped bounded shared-services
  path.
- [`279-plan-persistent-reset-foundation-batch.md`](./279-plan-persistent-reset-foundation-batch.md)
  is complete. The reopened `g02.015` lane now has one explicit first
  execution target instead of a broad persistence handoff.
- [`280-implement-generated-compose-persistent-reset-foundation.md`](./280-implement-generated-compose-persistent-reset-foundation.md)
  is complete. The first bounded `g02.015` lifecycle slice is now landed
  through generated-compose `container reset --keep-data`.
- [`281-plan-volume-inventory-batch.md`](./281-plan-volume-inventory-batch.md)
  is complete. The next `g02.015` widening step is now explicit again:
  volume inventory before transfer or hook orchestration.
- [`282-implement-container-data-list-foundation.md`](./282-implement-container-data-list-foundation.md)
  is complete. The bounded inventory surface is now landed through
  `effigy container data list`.
- [`283-plan-volume-transfer-batch.md`](./283-plan-volume-transfer-batch.md)
  is complete. The post-inventory widening decision is now explicit again:
  transfer comes before hook orchestration.
- [`284-implement-container-data-transfer-foundation.md`](./284-implement-container-data-transfer-foundation.md)
  is complete. The bounded generated-compose transfer surface is now landed.
- [`285-plan-post-transfer-data-lifecycle-batch.md`](./285-plan-post-transfer-data-lifecycle-batch.md)
  is complete. The post-transfer widening decision is now explicit again:
  media lifecycle comes before hook or seeding orchestration.
- [`286-implement-media-bind-mount-lifecycle-foundation.md`](./286-implement-media-bind-mount-lifecycle-foundation.md)
  is complete. Bounded generated-compose media bind-mount lifecycle is now
  landed through manifest-owned `data.media`.
- [`287-plan-post-media-data-orchestration-batch.md`](./287-plan-post-media-data-orchestration-batch.md)
  is complete. The post-media orchestration decision is now explicit again:
  `pull_production` comes before any seed-specific widening.
- [`288-implement-container-data-pull-production-foundation.md`](./288-implement-container-data-pull-production-foundation.md)
  is complete. Bounded generated-compose `data.pull_production` hook
  ownership is now landed.
- [`289-plan-post-pull-production-lane-closeout.md`](./289-plan-post-pull-production-lane-closeout.md)
  is complete. The lane decision is now explicit again: one real-project proof
  comes before closeout, and task-owned seeding stays on the shipped task and
  Rhai surface rather than widening product abstraction.
- [`290-prove-generated-compose-persistent-data-loop-in-one-real-project.md`](./290-prove-generated-compose-persistent-data-loop-in-one-real-project.md)
  is complete. The generated-compose persistent-data contract is now proven in
  one real project, and `g02.015` is closed on a trustworthy boundary.
- [`291-plan-dev-front-door-foundation-batch.md`](./291-plan-dev-front-door-foundation-batch.md)
  is complete. The broad `g02.013` dev-front-door roadmap now has one bounded
  first execution target instead of a vague daily-driver handoff.
- [`292-implement-managed-dev-task-and-lifecycle-foundation.md`](./292-implement-managed-dev-task-and-lifecycle-foundation.md)
  is complete. The first bounded `g02.013` product slice is now landed:
  manifest-owned managed dev-task metadata plus container lifecycle ownership
  inside the managed runtime.
- [`293-decide-post-lifecycle-foundation-follow-up.md`](./293-decide-post-lifecycle-foundation-follow-up.md)
  is complete. The next `g02.013` gap is now explicit: shell role before
  readiness UX or gateway auto-start.
- [`294-implement-managed-dev-shell-role-foundation.md`](./294-implement-managed-dev-shell-role-foundation.md)
  is complete. The managed dev runtime now has a bounded embedded shell-role
  path through the shipped primary-service container shell.
- [`295-decide-post-shell-role-follow-up.md`](./295-decide-post-shell-role-follow-up.md)
  is complete. The next `g02.013` gap is now explicit again: readiness UX
  before gateway auto-start.
- [`296-implement-managed-dev-readiness-ux-foundation.md`](./296-implement-managed-dev-readiness-ux-foundation.md)
  is complete. The managed dev runtime now has a bounded readiness UX contract
  through `managed.health_wait` plus `managed.ready_message`.
- [`297-decide-post-readiness-foundation-follow-up.md`](./297-decide-post-readiness-foundation-follow-up.md)
  is complete. The next `g02.013` gap is now explicit again: gateway auto-start
  before the final real-project proof.
- [`298-implement-managed-dev-gateway-auto-start-foundation.md`](./298-implement-managed-dev-gateway-auto-start-foundation.md)
  is complete. The managed dev runtime now has a bounded gateway auto-start
  path through the shipped `effigy gateway up` surface.
- [`299-decide-post-gateway-foundation-follow-up.md`](./299-decide-post-gateway-foundation-follow-up.md)
  is complete. The final `g02.013` gap is now explicit: one real-project proof
  before lane closeout.
- [`300-prove-managed-dev-front-door-in-one-real-project.md`](./300-prove-managed-dev-front-door-in-one-real-project.md)
  is complete. The managed dev front door is now proven in one real project,
  and `g02.013` is closed on a trustworthy boundary.

Staged next-lane card:

- [`301-implement-per-route-dns-ip-foundation.md`](./301-implement-per-route-dns-ip-foundation.md)
  is complete. The `g02.020` route-model seam is now real: routes can carry a
  per-route `dns_ip`, DNS resolution honors it, and the product path compiles
  cleanly with the new shape.
- [`302-decide-post-route-model-foundation-follow-up.md`](./302-decide-post-route-model-foundation-follow-up.md)
  is complete. The next bounded `g02.020` move is now explicit: loopback-IP
  allocation comes before HTTP post-start port discovery.

## Next Task

No active ready card.

Stop in planning and choose the next live roadmap deliberately.

## Next Task

Execute [`332-inventory-decodelabs-production-deployment-shape.md`](./332-inventory-decodelabs-production-deployment-shape.md).
- [`303-implement-loopback-ip-allocation-and-gateway-setup-foundation.md`](./303-implement-loopback-ip-allocation-and-gateway-setup-foundation.md)
  is the active ready card. `g02.020` was re-sequenced ahead of `g02.007` and
  `g02.019` on 2026-04-22 because the multi-project port-collision gap is now
  the most pressing operator friction (see
  `docs/logs/2026-04/22-190000-g02-020-re-sequencing-ahead-of-g02-007-and-g02-019.md`).

## Archive Rule

- closed or paused lane cards should move to `../archive/batch-cards/` once the
  lane no longer needs them in the active tree
- the active tree should stay focused on the live strict lanes rather than the
  full historical corpus
- use the governing spec plus roadmap to resolve the current ready card; this
  README is only the front door

## Next Task

No active ready card.

Stop in planning and choose the next milestone deliberately.
