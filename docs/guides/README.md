# Effigy Guides

Use this page to jump from a user goal to the right practical guide.

The intent is simple: show the obvious next read first, then hand off to deeper
pages only when you need more detail.

## Start Here

1. [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
   for the first useful commands.
2. [`055-everyday-workflows.md`](./055-everyday-workflows.md) for the common
   day-to-day paths Effigy should make easy.
3. [`058-demo-system-guide.md`](./058-demo-system-guide.md) for the demo
   registry, browser, terminal, and history surface.
4. [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
   for splitting `effigy.toml` into focused fragments.
5. [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
   for moving a repo from demo scripts and wrapper tasks onto the native demo
   surface.
6. [`022-manifest-cookbook.md`](./022-manifest-cookbook.md) for copy-paste
   `effigy.toml` patterns.
7. [`025-command-reference-matrix.md`](./025-command-reference-matrix.md) for
   the full command and flag surface.
8. [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md) when the
   job is "clone this repo here and bring it up."

## By Goal

### I want to get started fast

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)

### I want to clone a repo here and bring it up

- [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

### I want to run tasks and understand routing

- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

### I want to define or operate proof demos

- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

### I want to split `effigy.toml` into focused fragments

- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)

### I want testing, watch mode, init, or migrate

- [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)

### I want health checks, scans, or troubleshooting

- [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)

### I want env and schema handling to be explicit

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)

### I want CI, agents, or machine consumers

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)

If the goal is full repo adoption:
- let the `northstar-effigy` skill own bootstrap/scaffolding
- let Effigy own generic validation, JSON contracts, and release/runtime
  surfaces

### I want built-in release workflows

- [`051-release-orchestration.md`](./051-release-orchestration.md)
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`044-distribution-first-publish-execution-runbook.md`](./044-distribution-first-publish-execution-runbook.md)

### I want docs standards and maintenance rules

- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md)

## Fast Reference

### Common Commands

```sh
effigy tasks
effigy tasks --resolve test
effigy doctor --verbose
effigy test --plan
effigy watch --owner effigy --once test
effigy init
effigy migrate --from package.json
effigy --json tasks
```

### Env Resolution Cheatsheet

- Reusable named values live in top-level `[env]`.
- Run arrays can apply env in sequence with `{ env = "NAME" }`,
  `{ env = { KEY = "value" } }`, and `{ env_file = ".env.test" }`.
- Named env resolution order is:
  1. current catalog `[env]`
  2. process environment
  3. dotenv fallback (`.env` unless overridden)
- Cross-catalog env refs use `env = "<catalog-path>/<name>"`.
- Built-in cargo suites inherit manifest `CARGO_*` values during
  `effigy test`.

Details:
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)

## Full Guide Map

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

### Workflow Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
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
- [`053-release-wrapper-retirement-record-template.md`](./053-release-wrapper-retirement-record-template.md)
- [`054-release-checkpoint-log-template.md`](./054-release-checkpoint-log-template.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)

### Docs and Governance

- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`030-contributor-onboarding-15-minutes.md`](./030-contributor-onboarding-15-minutes.md)
- [`031-docs-navigation-cleanup.md`](./031-docs-navigation-cleanup.md)
- [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md)
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
- [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md)
- [`038-docs-ia-snapshot.md`](./038-docs-ia-snapshot.md)
- [`039-docs-drift-monitoring.md`](./039-docs-drift-monitoring.md)
- [`040-docs-archive-and-deprecation-policy.md`](./040-docs-archive-and-deprecation-policy.md)

### Distribution and Adoption

- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)
- [`043-wrapper-channel-evaluation-and-policy.md`](./043-wrapper-channel-evaluation-and-policy.md)
- [`044-distribution-first-publish-execution-runbook.md`](./044-distribution-first-publish-execution-runbook.md)
- [`045-vision-next-task-allowlist-maintenance.md`](./045-vision-next-task-allowlist-maintenance.md)
- [`046-vision-next-task-allowlist-pr-checklist-snippet.md`](./046-vision-next-task-allowlist-pr-checklist-snippet.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)

## Standards Used In These Guides

- Canonical JSON mode wording is `effigy --json <command>`.
- Canonical terms are `selector`, `routing`, and `deferral`.
- Practical guides should end with `Expected Outcome`, `Related Guides`, and
  `Next Step`.

References:
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
