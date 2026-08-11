# 019 - Watch, Init, and Migrate Foundation

Use this guide when you want to start a repo, migrate an existing script
surface, or rerun a task on file changes without inventing a local wrapper.

These commands solve different problems, but they share the same goal: reduce
the amount of repo-specific ceremony around common setup and iteration loops.

## Vision Alignment

- Primary tags: `ROUTE`, `OPERATE`, `MAINT`
- Target movement: watch/init/migrate onboarding flows stay bounded,
  predictable, and low-friction.

## Start Here

Choose the command by the friction you are trying to remove:

- Use `effigy init` when a repo has no `effigy.toml` yet.
- Use `effigy tasks migrate` when tasks already exist in `package.json` and should
  move into the manifest.
- Use `effigy watch` when a task already exists and should rerun on changes.

Common first commands:

```sh
effigy init
effigy tasks migrate --from package.json
effigy watch --owner effigy --once test
```

## `effigy watch`

Use watch mode when you want a bounded, explicit rerun loop instead of ad-hoc
watcher nesting.

The foundational contract is policy-first:

- owner policy is mandatory (`--owner <effigy|external>`)
- `external` fails fast to avoid nested watcher loops
- `effigy` enables file-triggered reruns with debounce and glob controls

### Usage

```sh
effigy watch --owner effigy --once test
effigy watch --owner effigy --debounce-ms 500 --include "src/**" --exclude "**/*.snap" test vitest user-service
effigy watch --owner external test
```

### Notes

- `--json` is supported for bounded runs only (`--once` or `--max-runs <N>`).
- Default excludes include `.git/**`, `node_modules/**`, and `target/**`.
- Effigy acquires a watch-owner lock scope per target (`task:watch:<target>`).
- Concurrent owners for the same target fail fast with lock diagnostics.
- If a watch lock must be cleared manually, use
  `effigy tasks unlock task:watch:<target>`.

## `effigy init`

Use `init` when the repo needs a clean starting point for Effigy or when you
want one bounded setup front door instead of hand-written bootstrap notes.

Plain `effigy init` now behaves in two modes:

- on a real TTY, it prompts phase-by-phase through the safe setup work it can
  actually perform
- on non-TTY paths, or when flags make the intent explicit, it stays
  deterministic and non-interactive

The baseline managed setup still covers:

- root `effigy.toml` when missing
- root `README.md` when missing
- managed `AGENTS.md` Effigy contract block
- project-local `.agents/skills/effigy`
- local `.effigy/` ignore policy

### Usage

```sh
effigy init
effigy init --check --json
effigy init --checklist --json
effigy init --apply-actions manifest.effigy_toml,graph_status.inspect --json
effigy init minimal --dry-run
effigy init northstar --force
```

### Safety

- plain prompt-driven behavior only happens on a real TTY without conflicting
  flags
- `--checklist --json` never writes; it reports the wider setup inventory with
  applicability, safety class, and recommended commands
- `--apply-actions` runs only the selected setup jobs and reports per-action
  outcomes
- named starters still own `--dry-run` and `--force`
- if `effigy.toml` already exists, starter emission fails unless `--force` is
  set
- if a root **`README.md`** already exists, starter emission **skips** that path
  by default so your project README is not replaced

## `effigy tasks migrate`

Use `migrate` when a repo already has useful `package.json` scripts and the
next step is to move them into Effigy without doing the whole rewrite by hand.

`migrate` imports ordinary scripts into `[tasks]`. The package `test` script
becomes `[test.suites].js`, preserving `effigy test` as the single test
orchestrator. Migration stays preview-first.

### Usage

```sh
effigy tasks migrate
effigy tasks migrate --script build --script test
effigy tasks migrate --apply
effigy tasks migrate --from ./frontend/package.json --apply --json
```

### Behavior

- Source is `package.json` by default (`--from` overrides).
- Preview mode does not write files.
- `--apply` writes only ready imports.
- Existing destination conflicts are skipped and reported with manual
  remediation guidance.
- `package.json` is never modified by migration.

## JSON Schemas

- `effigy.watch.v1` for bounded watch runs (`--json` + bounded mode)
- `effigy.init.v1` for baseline init and starter reports
- `effigy.init.checklist.v1` for wider setup inventory reports
- `effigy.init.actions.v1` for selected action execution reports
- `effigy.migrate.v1` for migration previews/applies

## Contracts Matrix

| Surface | Purpose | Test file(s) |
|---|---|---|
| `watch` behavior | owner policy, rerun loop, lock contention behavior | `src/tests/runner_tests.rs` |
| `watch` JSON payload | schema/version shape for bounded `--json` runs | `src/tests/json_contract_tests.rs` |
| CLI JSON envelope | top-level `effigy.command.v1` wrapping + error/remediation propagation | `tests/cli_output_tests.rs` |
| `init` behavior | scaffold write/force/dry-run semantics | `src/tests/runner_tests.rs` |
| `init` JSON payload | `effigy.init.v1` payload shape | `src/tests/json_contract_tests.rs` |
| `migrate` behavior | preview/apply/non-destructive import behavior | `src/tests/runner_tests.rs` |
| `migrate` JSON payload | `effigy.migrate.v1` payload shape | `src/tests/json_contract_tests.rs` |

## Expected Outcome

After this guide, you should be able to:

- pick the right command for first-time setup, script migration, or rerun loops
- use preview-first paths for init and migrate safely
- understand the lock and owner rules that keep watch mode predictable

## Related Guides

- DAG run/policy/lock baseline: [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
- Troubleshooting watch and lock failures: [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- CI recipes for JSON command automation: [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- Scenario-based adoption paths: [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)

## Next Step

After adopting any watch/init/migrate flow, move to
[`028-migration-quick-paths.md`](./028-migration-quick-paths.md) and convert
the next repo-specific bootstrap or watcher script into a documented Effigy
path.
