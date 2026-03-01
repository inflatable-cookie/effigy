# Effigy Docs

Effigy docs are organized by intent:

- `architecture/`: stable design and module boundaries.
- `contracts/`: machine-readable schema contracts and indexes.
- `guides/`: operator and contributor runbooks with examples.
- `roadmap/`: numbered implementation plans and checkpoints.
- `roadmap/backlog/`: unscheduled exploration items.
- `reports/`: dated validation artifacts and delivery logs.

## Recommended Reading Path

If you are new to the project:
1. [`../README.md`](../README.md) for quick start
2. [`guides/README.md`](./guides/README.md) for persona-based guide navigation
3. [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md) for command examples
4. [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md) for copy-paste manifest patterns
5. [`architecture/000-overview.md`](./architecture/000-overview.md) for system framing

If you are extending behavior:
1. [`guides/016-task-routing-precedence.md`](./guides/016-task-routing-precedence.md)
2. [`guides/017-json-output-contracts.md`](./guides/017-json-output-contracts.md)
3. [`architecture/010-package-map.md`](./architecture/010-package-map.md)
4. [`architecture/011-multiprocess-tui-config-contract.md`](./architecture/011-multiprocess-tui-config-contract.md)

## Guide Index

- [`guides/README.md`](./guides/README.md)
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
- [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
- [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md)
- [`guides/023-troubleshooting-and-failure-recipes.md`](./guides/023-troubleshooting-and-failure-recipes.md)
- [`guides/024-ci-and-automation-recipes.md`](./guides/024-ci-and-automation-recipes.md)
- [`guides/025-command-reference-matrix.md`](./guides/025-command-reference-matrix.md)
- [`guides/026-json-payload-examples.md`](./guides/026-json-payload-examples.md)
- [`guides/027-copy-paste-snippets.md`](./guides/027-copy-paste-snippets.md)
- [`guides/028-docs-flow-map.md`](./guides/028-docs-flow-map.md)

## JSON Contract Notes

- Canonical JSON mode is `effigy --json <command>`.
- Top-level envelope schema is `effigy.command.v1`.
- Payload schemas vary by command (`effigy.tasks.v1`, `effigy.doctor.v1`, etc.).
- Validation index lives at [`contracts/json-schema-index.json`](./contracts/json-schema-index.json).

## Recent Release Notes

- [`reports/2026-02-28-dag-watch-onboarding-release-note.md`](./reports/2026-02-28-dag-watch-onboarding-release-note.md)
- [`reports/2026-02-28-json-envelope-removal-release-note.md`](./reports/2026-02-28-json-envelope-removal-release-note.md)
- [`reports/2026-02-28-doctor-explain-mode-release-note.md`](./reports/2026-02-28-doctor-explain-mode-release-note.md)
