# Effigy Docs

Effigy docs are organized by intent:

- `architecture/`: stable design and module boundaries.
- `contracts/`: machine-readable schema contracts and indexes.
- `guides/`: operator and contributor runbooks.
- `roadmap/`: numbered implementation plans and checkpoints.
- `reports/`: dated validation artifacts and release notes.

## Recommended Reading Paths

New to Effigy:
1. [`../README.md`](../README.md)
2. [`guides/030-contributor-onboarding-15-minutes.md`](./guides/030-contributor-onboarding-15-minutes.md)
3. [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
4. [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md)
5. [`guides/025-command-reference-matrix.md`](./guides/025-command-reference-matrix.md)

Operating and debugging day-to-day:
1. [`guides/025-command-reference-matrix.md`](./guides/025-command-reference-matrix.md)
2. [`guides/023-troubleshooting-and-failure-recipes.md`](./guides/023-troubleshooting-and-failure-recipes.md)
3. [`guides/019-watch-init-migrate-phase-1.md`](./guides/019-watch-init-migrate-phase-1.md)

Automating and validating CI:
1. [`guides/017-json-output-contracts.md`](./guides/017-json-output-contracts.md)
2. [`guides/024-ci-and-automation-recipes.md`](./guides/024-ci-and-automation-recipes.md)
3. [`guides/026-json-payload-examples.md`](./guides/026-json-payload-examples.md)
4. [`guides/029-docs-qa-checklist-and-validation.md`](./guides/029-docs-qa-checklist-and-validation.md)

Maintaining docs process:
1. [`guides/037-documentation-contribution-playbook.md`](./guides/037-documentation-contribution-playbook.md)
2. [`guides/039-docs-drift-monitoring.md`](./guides/039-docs-drift-monitoring.md)
3. [`guides/040-docs-archive-and-deprecation-policy.md`](./guides/040-docs-archive-and-deprecation-policy.md)

## Guides Navigation

Primary navigation page:
- [`guides/README.md`](./guides/README.md)

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
- [`guides/019-watch-init-migrate-phase-1.md`](./guides/019-watch-init-migrate-phase-1.md)
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

Supplemental legacy navigation:
- [`guides/028-docs-flow-map.md`](./guides/028-docs-flow-map.md)

## JSON Contract Notes

- Canonical JSON mode is `effigy --json <command>`.
- Top-level envelope schema is `effigy.command.v1`.
- Validation index lives at [`contracts/json-schema-index.json`](./contracts/json-schema-index.json).

## Terminology Canon

Use the glossary terms consistently:
- `selector`
- `routing`
- `deferral`

Reference: [`guides/034-task-and-command-glossary.md`](./guides/034-task-and-command-glossary.md).

## Recent Release Notes

- [`reports/2026-02-28-dag-watch-onboarding-release-note.md`](./reports/2026-02-28-dag-watch-onboarding-release-note.md)
- [`reports/2026-02-28-json-envelope-removal-release-note.md`](./reports/2026-02-28-json-envelope-removal-release-note.md)
- [`reports/2026-02-28-doctor-explain-mode-release-note.md`](./reports/2026-02-28-doctor-explain-mode-release-note.md)
