# Effigy Guides

This page is the **practical guide map**: ordered journeys plus the full guide
inventory. For a shorter goal-based front door first, see
[`docs/README.md`](../README.md). For install and the shortest first-run path on
the project itself, see the repository [`README.md`](../../README.md).

The old problem here was simple: too many guides were presented at the same
level. This version narrows the front door down to a few real user journeys.

## Start Here

If you are new to Effigy, read these in order:

1. [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
2. [`055-everyday-workflows.md`](./055-everyday-workflows.md)
3. [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)

After that, choose one path below.

## By User Goal

### Get started and run work

Read:
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)

Then add:
- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
  only when routing still feels ambiguous

### Build or clean up `effigy.toml`

Read:
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)
- [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)

Use when:
- you are adopting Effigy in a repo
- you need env handling, includes, or migration help
- you want copy-paste manifest patterns

### Run a host-clean local stack

Read:
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
- [`065-underlay-starter.md`](./065-underlay-starter.md)
- [`067-catalog-services-reference.md`](./067-catalog-services-reference.md)
- [`071-catalog-service-authoring.md`](./071-catalog-service-authoring.md)
- [`069-workspace-host-integration.md`](./069-workspace-host-integration.md)
- [`070-per-machine-overlays-and-external-mounts.md`](./070-per-machine-overlays-and-external-mounts.md)

Use when:
- you want services, workspaces, gateway routing, and local domains
- you are adopting a bundle source or the service catalog
- you need mounts, isolation, or per-machine overlays

Use `071` only when you are changing the shipped catalog itself.

### Define and operate demos

Read:
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)

Use when:
- you want manifest-owned proof demos
- you need receipts, history, artifacts, or the demo browser

### Script Effigy with Rhai

Read:
- [`061-rhai-script-steps-guide.md`](./061-rhai-script-steps-guide.md)
- [`068-rhai-host-surface-audit.md`](./068-rhai-host-surface-audit.md)

Use when:
- shell glue is getting awkward
- you want typed host helpers instead of more bash

### Automate with JSON, CI, or agents

Read:
- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)

Use when:
- another tool is calling Effigy
- you need stable JSON
- you are adopting Effigy across multiple repos

### Release and distribute Effigy-managed software

Read:
- [`051-release-orchestration.md`](./051-release-orchestration.md) for the
  release cut workflow
- [`062-distribution-system-guide.md`](./062-distribution-system-guide.md) for
  distribution commands and evidence flows
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
  for maintainer policy and CI install rules
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
  for changelog-only work

Use when:
- you want release gates, prepare/execute flows, or distribution checks
- you need changelog extraction or release evidence

Older narrow runbooks for CI pinning, Homebrew-tap wiring, and first-publish
execution are now deprecated and kept only for historical detail.

### Health, testing, watch mode, and troubleshooting

Read:
- [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
- [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)

Use when:
- you need built-in health flows
- something is failing and you need the shortest path to diagnosis

### Work on the docs themselves

Read:
- [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`040-docs-archive-and-deprecation-policy.md`](./040-docs-archive-and-deprecation-policy.md)

## Reference Surfaces

Use these when you need lookup material, not onboarding:

- Full command reference: [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- Copy-paste snippets: [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)
- Glossary: [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
- Archive: [`archive/README.md`](./archive/README.md)

## Full Guide Inventory

This section is inventory, not a recommended reading order.

### Core Runtime

- [`010-path-installation-and-release.md`](./010-path-installation-and-release.md)
- [`011-output-widgets-and-colour-modes.md`](./011-output-widgets-and-colour-modes.md)
- [`012-dev-process-manager-tui.md`](./012-dev-process-manager-tui.md)
- [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`015-deferral-fallback-migration.md`](./015-deferral-fallback-migration.md)
- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
- [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)
- [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)

### Workflow and Feature Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)
- [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
- [`054-release-checkpoint-log-template.md`](./054-release-checkpoint-log-template.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md)
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`061-rhai-script-steps-guide.md`](./061-rhai-script-steps-guide.md)
- [`062-distribution-system-guide.md`](./062-distribution-system-guide.md)
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
- [`065-underlay-starter.md`](./065-underlay-starter.md)
- [`066-local-manifest-bundles.md`](./066-local-manifest-bundles.md)
- [`067-catalog-services-reference.md`](./067-catalog-services-reference.md)
- [`071-catalog-service-authoring.md`](./071-catalog-service-authoring.md)
- [`068-rhai-host-surface-audit.md`](./068-rhai-host-surface-audit.md)
- [`069-workspace-host-integration.md`](./069-workspace-host-integration.md)
- [`070-per-machine-overlays-and-external-mounts.md`](./070-per-machine-overlays-and-external-mounts.md)
- [`072-artifact-commands-guide.md`](./072-artifact-commands-guide.md)

### Docs and Governance

- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`030-contributor-onboarding-15-minutes.md`](./030-contributor-onboarding-15-minutes.md)
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md)
- [`038-docs-ia-snapshot.md`](./038-docs-ia-snapshot.md)
- [`040-docs-archive-and-deprecation-policy.md`](./040-docs-archive-and-deprecation-policy.md)

Deprecated but still link-stable:
- [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)
- [`039-docs-drift-monitoring.md`](./039-docs-drift-monitoring.md)
- [`archive/028-docs-flow-map.md`](./archive/028-docs-flow-map.md)
- [`archive/031-docs-navigation-cleanup.md`](./archive/031-docs-navigation-cleanup.md)
- [`archive/032-docs-consistency-sweep-and-changelog.md`](./archive/032-docs-consistency-sweep-and-changelog.md)
- [`archive/043-wrapper-channel-evaluation-and-policy.md`](./archive/043-wrapper-channel-evaluation-and-policy.md)
- [`archive/053-release-wrapper-retirement-record-template.md`](./archive/053-release-wrapper-retirement-record-template.md)

### Distribution and Adoption

- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
- [`062-distribution-system-guide.md`](./062-distribution-system-guide.md)

Deprecated but still link-stable:
- [`045-vision-next-task-allowlist-maintenance.md`](./045-vision-next-task-allowlist-maintenance.md)
- [`046-vision-next-task-allowlist-pr-checklist-snippet.md`](./046-vision-next-task-allowlist-pr-checklist-snippet.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)
- [`044-distribution-first-publish-execution-runbook.md`](./044-distribution-first-publish-execution-runbook.md)
