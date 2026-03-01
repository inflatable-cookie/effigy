# Effigy Guides

This is the practical landing page for Effigy runbooks and examples.

## Start Here

If you want one fast path:
1. [`028-docs-flow-map.md`](./028-docs-flow-map.md)
2. [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
3. [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
4. [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
5. [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
6. [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
7. [`026-json-payload-examples.md`](./026-json-payload-examples.md)
8. [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)

## By Persona

### 1) New User

Goal: run your first tasks and understand routing quickly.

Read in order:
1. [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
2. [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
3. [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)

Useful commands:

```sh
effigy --help
effigy tasks
effigy tasks --resolve test
effigy doctor
```

### 2) Daily Operator

Goal: run dev/test/watch workflows and recover quickly from failures.

Read in order:
1. [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md)
2. [`013-testing-orchestration.md`](./013-testing-orchestration.md)
3. [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)

Useful commands:

```sh
effigy test --plan
effigy watch --owner effigy --once test
effigy unlock --all
effigy doctor --verbose
```

### 3) CI Owner

Goal: build stable machine-readable automation and contract validation.

Read in order:
1. [`017-json-output-contracts.md`](./017-json-output-contracts.md)
2. [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
3. [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)

Useful commands:

```sh
effigy --json tasks
./scripts/check-json-contracts-ci.sh
./scripts/check-json-contracts.sh --fast --print-selected=json
./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json
```

### 4) Maintainer

Goal: change behavior safely across runtime, JSON contracts, and release flow.

Read in order:
1. [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
2. [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
3. [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
4. [`014-release-checklist-template.md`](./014-release-checklist-template.md)

Useful commands:

```sh
effigy doctor --verbose
effigy --json doctor
effigy --json test --plan
cargo test
```

## Topic Index

- Installation and release: [`010-path-installation-and-release.md`](./010-path-installation-and-release.md)
- Output rendering and colour modes: [`011-output-widgets-and-colour-modes.md`](./011-output-widgets-and-colour-modes.md)
- Managed dev process UI: [`012-dev-process-manager-tui.md`](./012-dev-process-manager-tui.md)
- Built-in testing orchestration: [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- Release checklist template: [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- Deferral migration strategy: [`015-deferral-fallback-migration.md`](./015-deferral-fallback-migration.md)
- Task routing precedence: [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- JSON output contracts: [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- Doctor explain mode: [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
- Watch/init/migrate phase-1: [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md)
- DAG lock/policy baseline: [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
- Command cookbook: [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- Manifest cookbook: [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- Troubleshooting recipes: [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- CI and automation recipes: [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- Command reference matrix: [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- JSON payload examples: [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- Copy/paste snippets: [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)
- Migration quick paths: [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)
- Docs flow map: [`028-docs-flow-map.md`](./028-docs-flow-map.md)
