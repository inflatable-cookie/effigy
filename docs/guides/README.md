# Effigy Guides

This is the navigation hub for practical runbooks.

## Start Here (Recommended Order)

1. [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md) - first run and daily command baseline.
2. [`022-manifest-cookbook.md`](./022-manifest-cookbook.md) - manifest patterns you can adapt.
3. [`025-command-reference-matrix.md`](./025-command-reference-matrix.md) - canonical command/flag matrix.
4. [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md) - symptom-first fixes.
5. [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md) - CI contract and automation workflows.
6. [`026-json-payload-examples.md`](./026-json-payload-examples.md) - payload examples for machine consumers.
7. [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md) - quick manifest scaffolds.
8. [`028-migration-quick-paths.md`](./028-migration-quick-paths.md) - scenario-based adoption paths.

## Standards Used In These Guides

- Canonical JSON mode wording: `effigy --json <command>`.
- Canonical terms: `selector`, `routing`, `deferral`.
- Guide endings are standardized with `Expected Outcome`, `Related Guides`, and `Next Step` in process/operator runbooks.

References:
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)

## Env Resolution Cheatsheet

Use this section as a fast reference for task env behavior.

- Reusable named entries live in top-level `[env]`.
  - Value form: `KEY = "value"`.
  - Grouped profile form: `cargo = [{ CARGO_HOME = "..." }, { CARGO_TARGET_DIR = "..." }]`.
- Run arrays can apply env in sequence with directives.
  - Named: `{ env = "CARGO_HOME" }` or `{ env = "cargo" }`.
  - Inline map: `{ env = { RUST_LOG = "debug" } }`.
  - Cross-catalog: `{ env = "../shared/CARGO_HOME" }`.
- Named env resolution order:
  1. `[env]` entry in the selected catalog (or referenced catalog for cross-catalog refs)
  2. process environment (same-catalog refs only)
  3. dotenv fallback (`.env` by default)
- Dotenv source override:
  - task-level: `tasks.<name>.env_file = ".env.test"` or `[".env.local", ".env.test"]`
  - run-step override: `{ env_file = ".env.local" }` or `{ env_file = [".env.local", ".env.test"] }`
  - ordered arrays are first-match wins by key.
- `env` and `env_file` run steps can be standalone state updates with no `run`/`task`.
- Token substitution is supported in env values:
  - `{project}` and `{repo}` resolve to the current catalog root path.
- Dotenv parsing accepts:
  - `KEY=value`
  - `export KEY=value`
  - quoted values with matching single or double quotes.
- Built-in cargo test suites inherit manifest `CARGO_*`:
  - `effigy test` auto-applies `[env]` `CARGO_*` entries to `cargo-nextest`/`cargo-test` execution.
  - `[test].cargo_env_match` controls matching scope: `executable-only`, `prefix-aware` (default), `shell-aware`.

Canonical source and examples:
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)

## By Persona

### New User

Read:
1. [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
2. [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
3. [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

### Daily Operator

Read:
1. [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
2. [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
3. [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md)

### CI Owner

Read:
1. [`017-json-output-contracts.md`](./017-json-output-contracts.md)
2. [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
3. [`026-json-payload-examples.md`](./026-json-payload-examples.md)

### Maintainer

Read:
1. [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
2. [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
3. [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)
4. [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)

## Docs Operations

- Docs QA checklist: [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- Contributor onboarding: [`030-contributor-onboarding-15-minutes.md`](./030-contributor-onboarding-15-minutes.md)
- Docs contribution playbook: [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md)
- Docs IA snapshot: [`038-docs-ia-snapshot.md`](./038-docs-ia-snapshot.md)
- Drift monitoring: [`039-docs-drift-monitoring.md`](./039-docs-drift-monitoring.md)
- Archive/deprecation policy: [`040-docs-archive-and-deprecation-policy.md`](./040-docs-archive-and-deprecation-policy.md)
- Distribution CI pinning + wrapper migration: [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- Homebrew tap + release automation: [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)
- Wrapper channel evaluation + policy: [`043-wrapper-channel-evaluation-and-policy.md`](./043-wrapper-channel-evaluation-and-policy.md)
- Distribution first-publish execution runbook: [`044-distribution-first-publish-execution-runbook.md`](./044-distribution-first-publish-execution-runbook.md)

## Supplemental

- Legacy docs flow map: [`028-docs-flow-map.md`](./028-docs-flow-map.md)
- Navigation cleanup note: [`031-docs-navigation-cleanup.md`](./031-docs-navigation-cleanup.md)
- Consistency sweep changelog: [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md)
- Style guide: [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- Glossary: [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
