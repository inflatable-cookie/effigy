# Effigy Docs

Effigy docs are organized by intent:

- `architecture/`: stable design and module boundaries.
- `contracts/`: machine-readable schema contracts and indexes.
- `guides/`: operator and contributor runbooks.
- `roadmaps/`: generation-sharded implementation plans and backlog.
- `logs/`: dated validation artifacts and release notes, segmented by month.
- `vision/`: long-horizon direction and target envelopes.
- `research/`: comparative tool research, competitive analysis, and translation memos.

## Recommended Reading Paths

New to Effigy:
1. [`../README.md`](../README.md)
2. [`guides/010-path-installation-and-release.md`](./guides/010-path-installation-and-release.md)
3. [`guides/030-contributor-onboarding-15-minutes.md`](./guides/030-contributor-onboarding-15-minutes.md)
4. [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
5. [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md)
6. [`guides/025-command-reference-matrix.md`](./guides/025-command-reference-matrix.md)

Adopting Effigy for AI agents and multi-repo rollout:
1. [`guides/047-agent-and-cross-repo-adoption.md`](./guides/047-agent-and-cross-repo-adoption.md)
2. [`guides/024-ci-and-automation-recipes.md`](./guides/024-ci-and-automation-recipes.md)
3. [`guides/041-distribution-ci-pinning-and-wrapper-migration.md`](./guides/041-distribution-ci-pinning-and-wrapper-migration.md)

Operating and debugging day-to-day:
1. [`guides/025-command-reference-matrix.md`](./guides/025-command-reference-matrix.md)
2. [`guides/023-troubleshooting-and-failure-recipes.md`](./guides/023-troubleshooting-and-failure-recipes.md)
3. [`guides/019-watch-init-migrate-foundation.md`](./guides/019-watch-init-migrate-foundation.md)
4. [`guides/README.md#env-resolution-cheatsheet`](./guides/README.md#env-resolution-cheatsheet)

Automating and validating CI:
1. [`guides/017-json-output-contracts.md`](./guides/017-json-output-contracts.md)
2. [`guides/024-ci-and-automation-recipes.md`](./guides/024-ci-and-automation-recipes.md)
3. [`guides/026-json-payload-examples.md`](./guides/026-json-payload-examples.md)
4. [`guides/029-docs-qa-checklist-and-validation.md`](./guides/029-docs-qa-checklist-and-validation.md)
5. [`contracts/README.md`](./contracts/README.md)

Maintaining docs process:
1. [`guides/037-documentation-contribution-playbook.md`](./guides/037-documentation-contribution-playbook.md)
2. [`guides/039-docs-drift-monitoring.md`](./guides/039-docs-drift-monitoring.md)
3. [`guides/040-docs-archive-and-deprecation-policy.md`](./guides/040-docs-archive-and-deprecation-policy.md)

## Guides Navigation

Primary navigation page:
- [`guides/README.md`](./guides/README.md)
- Env reference shortcut: [`guides/README.md#env-resolution-cheatsheet`](./guides/README.md#env-resolution-cheatsheet)

Guide structure standard:
- practical guides should end with `Expected Outcome`, `Related Guides`, and `Next Step`.
- docs process guides should keep explicit validation commands and cross-links.

## Guide Catalog

Core runtime guides (`010`-`020`):
- [`guides/010-path-installation-and-release.md`](./guides/010-path-installation-and-release.md)
- [`guides/011-output-widgets-and-colour-modes.md`](./guides/011-output-widgets-and-colour-modes.md)
- [`guides/012-dev-process-manager-tui.md`](./guides/012-dev-process-manager-tui.md)
- [`guides/013-testing-orchestration.md`](./guides/013-testing-orchestration.md)
- [`guides/014-release-checklist-template.md`](./guides/014-release-checklist-template.md)
- [`guides/015-deferral-fallback-migration.md`](./guides/015-deferral-fallback-migration.md)
- [`guides/016-task-routing-precedence.md`](./guides/016-task-routing-precedence.md)
- [`guides/017-json-output-contracts.md`](./guides/017-json-output-contracts.md)
- [`guides/018-doctor-explain-mode.md`](./guides/018-doctor-explain-mode.md)
- [`guides/019-watch-init-migrate-foundation.md`](./guides/019-watch-init-migrate-foundation.md)
- [`guides/020-dag-lock-policy-baseline.md`](./guides/020-dag-lock-policy-baseline.md)

Workflow and examples (`021`-`028`):
- [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
- [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md)
- [`guides/025-command-reference-matrix.md`](./guides/025-command-reference-matrix.md)
- [`guides/023-troubleshooting-and-failure-recipes.md`](./guides/023-troubleshooting-and-failure-recipes.md)
- [`guides/024-ci-and-automation-recipes.md`](./guides/024-ci-and-automation-recipes.md)
- [`guides/026-json-payload-examples.md`](./guides/026-json-payload-examples.md)
- [`guides/027-copy-paste-snippets.md`](./guides/027-copy-paste-snippets.md)
- [`guides/028-migration-quick-paths.md`](./guides/028-migration-quick-paths.md)

Docs operations (`029`-`040`):
- [`guides/029-docs-qa-checklist-and-validation.md`](./guides/029-docs-qa-checklist-and-validation.md)
- [`guides/030-contributor-onboarding-15-minutes.md`](./guides/030-contributor-onboarding-15-minutes.md)
- [`guides/031-docs-navigation-cleanup.md`](./guides/031-docs-navigation-cleanup.md)
- [`guides/032-docs-consistency-sweep-and-changelog.md`](./guides/032-docs-consistency-sweep-and-changelog.md)
- [`guides/033-style-and-terminology-guide.md`](./guides/033-style-and-terminology-guide.md)
- [`guides/034-task-and-command-glossary.md`](./guides/034-task-and-command-glossary.md)
- [`guides/035-guide-ownership-and-update-triggers.md`](./guides/035-guide-ownership-and-update-triggers.md)
- [`guides/036-release-notes-authoring-template-and-examples.md`](./guides/036-release-notes-authoring-template-and-examples.md)
- [`guides/037-documentation-contribution-playbook.md`](./guides/037-documentation-contribution-playbook.md)
- [`guides/038-docs-ia-snapshot.md`](./guides/038-docs-ia-snapshot.md)
- [`guides/039-docs-drift-monitoring.md`](./guides/039-docs-drift-monitoring.md)
- [`guides/040-docs-archive-and-deprecation-policy.md`](./guides/040-docs-archive-and-deprecation-policy.md)

Distribution and governance extensions (`041`-`045`):
- [`guides/041-distribution-ci-pinning-and-wrapper-migration.md`](./guides/041-distribution-ci-pinning-and-wrapper-migration.md)
- [`guides/042-homebrew-tap-and-release-automation.md`](./guides/042-homebrew-tap-and-release-automation.md)
- [`guides/043-wrapper-channel-evaluation-and-policy.md`](./guides/043-wrapper-channel-evaluation-and-policy.md)
- [`guides/044-distribution-first-publish-execution-runbook.md`](./guides/044-distribution-first-publish-execution-runbook.md)
- [`guides/045-vision-next-task-allowlist-maintenance.md`](./guides/045-vision-next-task-allowlist-maintenance.md)
- [`guides/046-vision-next-task-allowlist-pr-checklist-snippet.md`](./guides/046-vision-next-task-allowlist-pr-checklist-snippet.md)

Supplemental legacy navigation:
- [`guides/028-docs-flow-map.md`](./guides/028-docs-flow-map.md)

## JSON Contract Notes

- Canonical JSON mode is `effigy --json <command>`.
- Top-level envelope schema is `effigy.command.v1`.
- Validation index lives at [`contracts/json-schema-index.json`](./contracts/json-schema-index.json).
- Ownership/drift policy lives at [`contracts/README.md`](./contracts/README.md).

## Research Notes

- Research index: [`research/README.md`](./research/README.md)
- Research implementation bridge: [`research/master-index.md`](./research/master-index.md), [`research/research-to-implementation-playbook.md`](./research/research-to-implementation-playbook.md)
- Research tracks: [`research/value-tracks/`](./research/value-tracks/)
- Tool dossiers: [`research/tool-dossiers/`](./research/tool-dossiers/)

## Vision Notes

- Vision index: [`vision/README.md`](./vision/README.md)
- Vision rollout history: [`vision/history/README.md`](./vision/history/README.md)

## Terminology Canon

Use the glossary terms consistently:
- `selector`
- `routing`
- `deferral`

Reference: [`guides/034-task-and-command-glossary.md`](./guides/034-task-and-command-glossary.md).

## Recent Release Notes

- [`logs/2026-02/28-090000-dag-watch-onboarding-release-note.md`](./logs/2026-02/28-090000-dag-watch-onboarding-release-note.md)
- [`logs/2026-02/28-090800-json-envelope-removal-release-note.md`](./logs/2026-02/28-090800-json-envelope-removal-release-note.md)
- [`logs/2026-02/28-090100-doctor-explain-mode-release-note.md`](./logs/2026-02/28-090100-doctor-explain-mode-release-note.md)
