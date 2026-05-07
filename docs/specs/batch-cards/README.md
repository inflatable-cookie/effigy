# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning
- keep the active tree focused on live or near-live cards
- archive stale cards once their lane is closed or paused cleanly
- do not use this index as a graveyard dump of historical cards

## Archive Boundary

Cards `430` and older have been archived under
[`../archive/batch-cards/`](../archive/batch-cards/).

The active batch-card tree now starts with `g04` rollover card `431`.

## Current Live Chain

- [`431-audit-runtime-architecture-and-open-g04.md`](./431-audit-runtime-architecture-and-open-g04.md)
  is complete. It landed the runtime architecture sanity audit and opened
  `g04`.

- [`432-scaffold-execution-pipeline-ownership-lane.md`](./432-scaffold-execution-pipeline-ownership-lane.md)
  is complete. It opened the first `g04.002` implementation lane and selected
  the dispatch-plan foundation slice.

- [`433-add-execution-dispatch-plan-foundation.md`](./433-add-execution-dispatch-plan-foundation.md)
  is complete. It added pure dispatch-plan types to `effigy-execution`.

- [`434-select-next-execution-planning-slice.md`](./434-select-next-execution-planning-slice.md)
  is complete. It selected the next bounded `g04.002` implementation slice.

- [`435-move-execution-preflight-input-behind-dispatch-plan.md`](./435-move-execution-preflight-input-behind-dispatch-plan.md)
  is complete. It moved runner preflight input behind the shared dispatch plan.

- [`436-select-discovery-or-selection-planning-slice.md`](./436-select-discovery-or-selection-planning-slice.md)
  is complete. It selected the next discovery or selection planning slice.

- [`437-add-execution-discovery-plan-foundation.md`](./437-add-execution-discovery-plan-foundation.md)
  is complete. It added the first shared discovery plan shape.

- [`438-select-selection-input-or-catalog-handoff-slice.md`](./438-select-selection-input-or-catalog-handoff-slice.md)
  is complete. It selected the next selection input or catalog handoff slice.

- [`439-add-execution-selection-plan-summary.md`](./439-add-execution-selection-plan-summary.md)
  is complete. It added the shared selection plan summary.

- [`440-select-binding-input-or-selected-task-adapter-slice.md`](./440-select-binding-input-or-selected-task-adapter-slice.md)
  is complete. It selected the next binding input or selected-task adapter
  slice.

- [`441-add-execution-binding-plan-summary.md`](./441-add-execution-binding-plan-summary.md)
  is complete. It added the shared binding plan summary.

- [`442-select-dispatch-stage-or-runtime-activation-handoff.md`](./442-select-dispatch-stage-or-runtime-activation-handoff.md)
  is complete. It selected closeout and runtime activation handoff.

- [`443-close-execution-pipeline-ownership-and-handoff-runtime-activation.md`](./443-close-execution-pipeline-ownership-and-handoff-runtime-activation.md)
  is complete. It closed `g04.002` and handed off to runtime activation.

- [`444-scaffold-runtime-activation-pipeline-lane.md`](./444-scaffold-runtime-activation-pipeline-lane.md)
  is complete. It scaffolded the `g04.003` runtime activation implementation
  lane.

- [`445-scaffold-effigy-runtime-plan-crate.md`](./445-scaffold-effigy-runtime-plan-crate.md)
  is complete. It added the first dependency-light runtime activation plan
  crate.

- [`446-select-first-runtime-plan-runner-integration.md`](./446-select-first-runtime-plan-runner-integration.md)
  is complete. It selected the first runner integration point for runtime
  planning.

- [`447-wire-runtime-activation-plan-into-exec-surface.md`](./447-wire-runtime-activation-plan-into-exec-surface.md)
  is complete. It wired runtime activation planning into `effigy exec`.

- [`448-select-next-runtime-activation-integration.md`](./448-select-next-runtime-activation-integration.md)
  is complete. It selected the next runtime activation integration point.

- [`449-wire-runtime-activation-plan-into-db-seed.md`](./449-wire-runtime-activation-plan-into-db-seed.md)
  is complete. It wired runtime activation planning into DB seed runtime prep.

- [`450-select-deferral-or-standard-task-runtime-integration.md`](./450-select-deferral-or-standard-task-runtime-integration.md)
  is complete. It selected deferral as the next activation integration point.

- [`451-wire-runtime-activation-plan-into-deferral.md`](./451-wire-runtime-activation-plan-into-deferral.md)
  is complete. It wired runtime activation planning into deferred container
  execution.

- [`452-wire-runtime-activation-plan-into-standard-task-activation.md`](./452-wire-runtime-activation-plan-into-standard-task-activation.md)
  is complete. It wired runtime activation planning into standard task
  activation.

- [`453-wire-runtime-activation-plan-into-managed-task-activation.md`](./453-wire-runtime-activation-plan-into-managed-task-activation.md)
  is complete. It wired runtime activation planning into managed task
  activation.

- [`454-select-runtime-prep-stage-migration-slice.md`](./454-select-runtime-prep-stage-migration-slice.md)
  is complete. It selected the first runtime-prep side-effect stage migration.

- [`455-move-runtime-prep-activation-executor-behind-plan.md`](./455-move-runtime-prep-activation-executor-behind-plan.md)
  is complete. It moves task activation side effects behind the activation
  plan.

- [`456-extract-runtime-policy-validation-stage.md`](./456-extract-runtime-policy-validation-stage.md)
  is complete. It extracts runtime policy validation into the first named
  activation stage.

- [`457-extract-runtime-running-state-and-ensure-running-stages.md`](./457-extract-runtime-running-state-and-ensure-running-stages.md)
  is complete. It extracts running-state and ensure-running activation stages.

- [`458-extract-runtime-mount-preparation-stage.md`](./458-extract-runtime-mount-preparation-stage.md)
  is complete. It extracts mount preparation into a named activation stage.

- [`459-extract-runtime-compose-up-stage.md`](./459-extract-runtime-compose-up-stage.md)
  is complete. It extracts compose up into a named activation stage.

- [`460-extract-runtime-exec-readiness-stage.md`](./460-extract-runtime-exec-readiness-stage.md)
  is complete. It extracts exec readiness into a named activation stage.

- [`461-extract-runtime-alias-reconciliation-stage.md`](./461-extract-runtime-alias-reconciliation-stage.md)
  is complete. It extracts alias reconciliation into a named activation stage.

- [`462-extract-runtime-gateway-readiness-stage.md`](./462-extract-runtime-gateway-readiness-stage.md)
  is complete. It extracts gateway readiness into a named activation stage.

- [`463-extract-runtime-lease-refresh-stage.md`](./463-extract-runtime-lease-refresh-stage.md)
  is complete. It extracts lease refresh into a named activation stage.

- [`464-decide-runtime-activation-stage-extraction-closeout.md`](./464-decide-runtime-activation-stage-extraction-closeout.md)
  is complete. It decides the next runtime activation slice.

- [`465-split-runtime-prep-stage-modules.md`](./465-split-runtime-prep-stage-modules.md)
  is complete. It splits runtime prep into stage-owned modules.

- [`466-wire-runtime-activation-plan-into-workspace-sessions.md`](./466-wire-runtime-activation-plan-into-workspace-sessions.md)
  is complete. It wires runtime activation planning into workspace sessions.

- [`467-select-next-runtime-activation-caller-migration.md`](./467-select-next-runtime-activation-caller-migration.md)
  is complete. It selects Rhai container-sensitive execution as the next
  runtime activation caller migration.

- [`468-route-rhai-container-exec-through-runtime-activation.md`](./468-route-rhai-container-exec-through-runtime-activation.md)
  is complete. It routes Rhai container exec callbacks through runtime
  activation.

- [`469-decide-runtime-activation-pipeline-closeout.md`](./469-decide-runtime-activation-pipeline-closeout.md)
  is complete. It closes `g04.003` and hands off to `g04.004`.

- [`470-scaffold-container-operation-pipeline-lane.md`](./470-scaffold-container-operation-pipeline-lane.md)
  is complete. It opens the container operation pipeline lane.

- [`471-add-container-ops-lifecycle-plan-foundation.md`](./471-add-container-ops-lifecycle-plan-foundation.md)
  is complete. It adds the first lifecycle operation plan substrate.

- [`472-wire-lifecycle-operation-plans-into-runner-glue.md`](./472-wire-lifecycle-operation-plans-into-runner-glue.md)
  is complete. It wires lifecycle operation plans into runner glue.

- [`473-select-next-container-operation-family.md`](./473-select-next-container-operation-family.md)
  is complete. It selects read-only status/logs/stats operations next.

- [`474-add-container-read-operation-plans.md`](./474-add-container-read-operation-plans.md)
  is complete. It adds read-only operation plans.

- [`475-wire-read-operation-plans-into-runtime-glue.md`](./475-wire-read-operation-plans-into-runtime-glue.md)
  is complete. It wires read operation plans into runtime glue.

- [`476-select-next-container-operation-slice.md`](./476-select-next-container-operation-slice.md)
  is complete. It selects exec/shell operations next.

- [`477-add-container-exec-shell-operation-plans.md`](./477-add-container-exec-shell-operation-plans.md)
  is complete. It adds exec/shell operation plans.

- [`478-wire-exec-shell-operation-plans-into-runner-glue.md`](./478-wire-exec-shell-operation-plans-into-runner-glue.md)
  is complete. It wires exec/shell operation plans into runner glue.

- [`479-select-data-cache-or-manager-migration.md`](./479-select-data-cache-or-manager-migration.md)
  is complete. It selects data/cache operation planning next.

- [`480-add-container-data-cache-operation-plans.md`](./480-add-container-data-cache-operation-plans.md)
  is complete. It adds data/cache operation plans.

- [`481-wire-data-cache-operation-plans-into-runtime-glue.md`](./481-wire-data-cache-operation-plans-into-runtime-glue.md)
  is complete. It wires data/cache operation plans into runtime glue.

- [`482-select-container-manager-migration-or-closeout.md`](./482-select-container-manager-migration-or-closeout.md)
  is complete. It selects manager-backed migration for `g04.004`.

- [`483-add-manager-compose-invocation-plan-foundation.md`](./483-add-manager-compose-invocation-plan-foundation.md)
  is complete. It adds the manager-owned compose invocation plan substrate.

- [`484-wire-manager-compose-plan-into-runtime-read-callers.md`](./484-wire-manager-compose-plan-into-runtime-read-callers.md)
  is complete. It wires manager compose plans into read-only runtime callers.

- [`485-wire-manager-compose-plan-into-lifecycle-down-reset.md`](./485-wire-manager-compose-plan-into-lifecycle-down-reset.md)
  is complete. It wires manager compose plans into lifecycle down/reset.

- [`486-select-exec-shell-or-data-cache-manager-migration.md`](./486-select-exec-shell-or-data-cache-manager-migration.md)
  is complete. It selects captured exec as the next manager migration slice.

- [`487-wire-manager-compose-plan-into-captured-exec.md`](./487-wire-manager-compose-plan-into-captured-exec.md)
  is complete. It wires manager compose plans into captured exec.

- [`488-wire-manager-compose-plan-into-interactive-shell.md`](./488-wire-manager-compose-plan-into-interactive-shell.md)
  is complete. It wires manager compose plans into interactive shell.

- [`489-select-attached-session-or-data-cache-manager-migration.md`](./489-select-attached-session-or-data-cache-manager-migration.md)
  is complete. It selects attached-session manager migration.

- [`490-wire-manager-compose-plan-into-attached-session.md`](./490-wire-manager-compose-plan-into-attached-session.md)
  is complete. It wires manager compose plans into attached session.

- [`491-select-data-cache-or-gateway-support-manager-migration.md`](./491-select-data-cache-or-gateway-support-manager-migration.md)
  is complete. It selects data pull-production runtime bring-up.

- [`492-wire-manager-compose-plan-into-data-pull-production.md`](./492-wire-manager-compose-plan-into-data-pull-production.md)
  is complete. It wires manager compose plans into data pull-production.

- [`493-select-gateway-support-image-cleanup-or-up-migration.md`](./493-select-gateway-support-image-cleanup-or-up-migration.md)
  is complete. It selects `container up` migration.

- [`494-wire-manager-compose-plan-into-container-up.md`](./494-wire-manager-compose-plan-into-container-up.md)
  is complete. It wires manager compose plans into `container up`.

- [`495-select-gateway-support-image-cleanup-or-shared-service-migration.md`](./495-select-gateway-support-image-cleanup-or-shared-service-migration.md)
  is complete. It selects gateway TCP alias host migration.

- [`496-wire-manager-compose-plan-into-gateway-tcp-alias-hosts.md`](./496-wire-manager-compose-plan-into-gateway-tcp-alias-hosts.md)
  is complete. It wires manager compose plans into gateway TCP alias host updates.

- [`497-select-shared-service-or-generated-image-cleanup-migration.md`](./497-select-shared-service-or-generated-image-cleanup-migration.md)
  is complete. It selects shared-service bring-up migration.

- [`498-wire-manager-compose-plan-into-shared-service-bring-up.md`](./498-wire-manager-compose-plan-into-shared-service-bring-up.md)
  is complete. It wires manager compose plans into shared-service bring-up.

- [`499-wire-manager-runtime-plan-into-generated-image-cleanup.md`](./499-wire-manager-runtime-plan-into-generated-image-cleanup.md)
  is complete. It wires manager runtime plans into generated image cleanup.

- [`500-review-container-operation-drift-and-closeout.md`](./500-review-container-operation-drift-and-closeout.md)
  is complete. It reviews remaining drift and selects final runner helper cleanup.

- [`501-remove-final-runner-compose-runtime-helper-drift.md`](./501-remove-final-runner-compose-runtime-helper-drift.md)
  is complete. It removes the final runner-owned compose/runtime helper drift.

- [`502-scaffold-data-seed-dump-pipeline-lane.md`](./502-scaffold-data-seed-dump-pipeline-lane.md)
  is complete. It scaffolds the data seed/dump pipeline lane.

- [`503-scaffold-effigy-data-crate-and-target-model.md`](./503-scaffold-effigy-data-crate-and-target-model.md)
  is complete. It scaffolds the first `effigy-data` crate and pure target model.

- [`504-move-database-command-rendering-into-effigy-data.md`](./504-move-database-command-rendering-into-effigy-data.md)
  is complete. It moves postgres/mariadb command rendering into `effigy-data`.

- [`505-centralize-data-artifact-reference-classification.md`](./505-centralize-data-artifact-reference-classification.md)
  is complete. It centralizes data artifact reference classification.

- [`506-move-logical-data-target-model-into-effigy-data.md`](./506-move-logical-data-target-model-into-effigy-data.md)
  is complete. It moves logical data target identity to `effigy-data`.

- [`507-move-seed-source-normalization-into-effigy-data.md`](./507-move-seed-source-normalization-into-effigy-data.md)
  is complete. It moves seed source normalization into `effigy-data`.

- [`508-move-dump-destination-normalization-into-effigy-data.md`](./508-move-dump-destination-normalization-into-effigy-data.md)
  is complete. It moves dump destination normalization into `effigy-data`.

- [`509-add-data-artifact-handoff-plan-foundation.md`](./509-add-data-artifact-handoff-plan-foundation.md)
  is complete. It adds pure data artifact handoff planning.

- [`510-wire-data-artifact-handoff-plans-into-runner-glue.md`](./510-wire-data-artifact-handoff-plans-into-runner-glue.md)
  is complete. It wires data artifact handoff plans into runner glue.

- [`511-select-artifact-staging-migration-or-foundation-closeout.md`](./511-select-artifact-staging-migration-or-foundation-closeout.md)
  is complete. It selects artifact staging migration or foundation closeout.

- [`512-add-seed-artifact-staging-plan-foundation.md`](./512-add-seed-artifact-staging-plan-foundation.md)
  is complete. It adds pure seed artifact staging planning.

- [`513-close-data-pipeline-foundation-pass.md`](./513-close-data-pipeline-foundation-pass.md)
  is complete. It closes the first data pipeline foundation pass.

- [`514-add-data-target-manifest-adapter-foundation.md`](./514-add-data-target-manifest-adapter-foundation.md)
  is complete. It adds the data target manifest adapter foundation.

- [`515-add-data-target-selection-plan.md`](./515-add-data-target-selection-plan.md)
  is complete. It adds shared data target selection planning.

- [`516-add-data-service-selection-plan-foundation.md`](./516-add-data-service-selection-plan-foundation.md)
  is complete. It adds shared database service selection planning.

- [`517-select-data-pipeline-closeout-or-runner-module-split.md`](./517-select-data-pipeline-closeout-or-runner-module-split.md)
  is complete. It selected one bounded runner module split before data
  closeout.

- [`518-split-container-data-prompt-module.md`](./518-split-container-data-prompt-module.md)
  is complete. It split container data prompt policy and rendering out of the
  main data command module.

- [`519-close-data-seed-dump-pipeline-and-open-rhai-lane.md`](./519-close-data-seed-dump-pipeline-and-open-rhai-lane.md)
  is complete. It closed `g04.005` and opened the Rhai host lane.

- [`520-audit-rhai-host-surface-and-scaffold-lane.md`](./520-audit-rhai-host-surface-and-scaffold-lane.md)
  is complete. It audits Rhai host surfaces and selects the first split.

- [`521-split-rhai-pure-utility-host-modules.md`](./521-split-rhai-pure-utility-host-modules.md)
  is complete. It splits pure utility module builders out of `host_api.rs`.

- [`522-split-rhai-filesystem-host-module.md`](./522-split-rhai-filesystem-host-module.md)
  is complete. It splits filesystem helpers out of `host_api.rs`.

- [`523-split-rhai-process-http-search-host-modules.md`](./523-split-rhai-process-http-search-host-modules.md)
  is complete. It splits process, HTTP, and search helpers out of
  `host_api.rs`.

- [`524-split-rhai-feature-callback-host-modules.md`](./524-split-rhai-feature-callback-host-modules.md)
  is complete. It splits non-container callback feature modules out of
  `host_api.rs`.

- [`525-split-rhai-container-host-module.md`](./525-split-rhai-container-host-module.md)
  is complete. It splits the container module builder out of `host_api.rs`.

- [`526-split-rhai-exec-host-module-and-review-callback-purity.md`](./526-split-rhai-exec-host-module-and-review-callback-purity.md)
  is complete. It splits the exec module builder and reviews callback purity.

- [`527-route-rhai-container-exec-callback-through-operation-surface.md`](./527-route-rhai-container-exec-callback-through-operation-surface.md)
  is complete. It routes Rhai container exec callbacks through the operation
  surface.

- [`528-close-rhai-host-api-split-and-callback-purity.md`](./528-close-rhai-host-api-split-and-callback-purity.md)
  is complete. It closes the Rhai host split lane.

- [`529-scaffold-effective-container-policy-decomposition-lane.md`](./529-scaffold-effective-container-policy-decomposition-lane.md)
  is complete. It opens the effective container policy decomposition lane.

- [`530-extract-effective-container-policy-model-module.md`](./530-extract-effective-container-policy-model-module.md)
  is complete. It extracts the effective container policy model module.

- [`531-extract-effective-container-policy-project-module.md`](./531-extract-effective-container-policy-project-module.md)
  is complete. It extracts policy project-name helpers.

- [`532-extract-effective-container-policy-validation-module.md`](./532-extract-effective-container-policy-validation-module.md)
  is complete. It extracts policy validation helpers.

- [`533-extract-inline-workspace-policy-module.md`](./533-extract-inline-workspace-policy-module.md)
  is complete. It extracts inline workspace policy helpers.

- [`534-extract-runtime-dns-policy-module.md`](./534-extract-runtime-dns-policy-module.md)
  is complete. It extracts runtime DNS policy helpers.

- [`535-extract-generated-compose-eject-module.md`](./535-extract-generated-compose-eject-module.md)
  is complete. It extracts generated compose eject helpers.

- [`536-extract-container-policy-load-module.md`](./536-extract-container-policy-load-module.md)
  is complete. It extracts container policy loading and assembly.

- [`537-extract-workspace-host-integration-module.md`](./537-extract-workspace-host-integration-module.md)
  is complete. It extracts workspace host-integration helpers.

- [`538-extract-workspace-library-mounts-module.md`](./538-extract-workspace-library-mounts-module.md)
  is complete. It extracts workspace library mount helpers.

- [`539-extract-workspace-isolation-mounts-module.md`](./539-extract-workspace-isolation-mounts-module.md)
  is complete. It extracts workspace isolation mount helpers.

- [`540-extract-workspace-compose-rewrite-module.md`](./540-extract-workspace-compose-rewrite-module.md)
  is complete. It extracts workspace compose rewrite helpers.

- [`541-extract-generated-compose-source-module.md`](./541-extract-generated-compose-source-module.md)
  is complete. It extracts generated compose source resolution.

- [`542-extract-container-exec-implementation-module.md`](./542-extract-container-exec-implementation-module.md)
  is complete. It extracts the container exec implementation behind a facade.

- [`543-extract-container-exec-parse-module.md`](./543-extract-container-exec-parse-module.md)
  is ready. It extracts container exec parsing.

## Next Task

Start card
[`543-extract-container-exec-parse-module.md`](./543-extract-container-exec-parse-module.md).
