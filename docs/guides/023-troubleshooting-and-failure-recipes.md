# 023 - Troubleshooting and Failure Recipes

This guide maps common Effigy failures to focused diagnosis steps and concrete fixes.

Use [`025-command-reference-matrix.md`](./025-command-reference-matrix.md) for full command syntax and flag details.


## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`
- Target movement: failure triage paths reduce time-to-diagnosis and keep remediation steps action-oriented.

## When To Use This

Reach for this guide when:

- a task does not resolve where you expected
- a built-in command fails and you need the shortest diagnosis path
- watch mode, locks, env resolution, or deferral behavior feels unclear
- CI is failing and you need the human-first version of the fix path

## 1) Fast Triage Flow

Start with:

```sh
effigy tasks --resolve test
effigy doctor --verbose
```

For CI/automation cases, capture machine output:

```sh
effigy --json tasks --resolve test
effigy --json doctor
```

## 2) Task Resolution Failures

### Symptom: `task catalog prefix <...> not found`

Diagnosis:

```sh
effigy tasks
```

Fix:
- use a real catalog alias from `effigy tasks`, or
- update `[catalog].alias` in the target `effigy.toml`, then retry.

### Symptom: `task <...> is not defined in effective catalogs`

Diagnosis:

```sh
effigy tasks --task <task-name>
effigy tasks --resolve <task-name>
```

Fix:
- add the missing task under `[tasks]` in the right catalog, or
- declare that catalog under root `[catalog.members]` or a typed catalog mount,
  or
- call an existing prefixed task (`<catalog>/<task>`).

### Symptom: `task <...> is ambiguous; matched multiple catalogs`

Diagnosis:

```sh
effigy tasks --resolve <task-name>
```

Fix:
- run with explicit prefix (`<catalog>/<task>`), or
- run from a deeper directory to trigger nearest in-scope resolution.

### Symptom: workspace root defines duplicate shim tasks for child-owned commands

Example:
- root `effigy.toml` defines `db:reset`
- one child catalog already uniquely owns `db:reset`

Diagnosis:

```sh
effigy tasks --task db:reset
effigy tasks --resolve db:reset
```

Fix:
- remove the duplicate root shim when a single child catalog already owns the task
- keep the task in the owning child catalog and let unprefixed routing resolve it
- add a root task only if the root is introducing distinct orchestration behavior
  rather than mirroring a child task

## 3) Catalog Discovery and Manifest Issues

### Symptom: `skill` source is rejected before execution

Diagnosis:

```sh
effigy skill tasks --path <SKILL_DIR|EFFIGY_TOML> --json
effigy skill --help
```

Fix:

- pass the directory that directly owns `effigy.toml`, or the manifest itself
- keep all composed includes and bundle roots inside the canonical skill root
- keep every reachable Rhai path inside the canonical skill root; absolute and
  symlink paths are accepted only when their canonical target stays inside it
- remove `[catalog.members]`; the v1 skill surface accepts one isolated catalog
- make the selected task and every nested source task explicitly host-compatible
  (`run_in = "host"`) and remove system/workspace or manifest-secrets binding
- replace managed/TUI/concurrent task shapes with standard host run sequences,
  or keep those tasks in the consumer repository

Do not replace `--path` with `--repo`. `--path` selects task code; `--repo`
selects the consumer where runtime effects belong.

### Symptom: `skill run` cannot resolve a consumer target

Run from inside the consumer repository or pass it explicitly:

```sh
effigy skill run --path <SKILL_DIR> <SELECTOR> --repo /path/to/consumer --json
```

The JSON result should show distinct `source.root` and `target.root` values,
plus the original `invocation_cwd` and target-owned `execution_cwd`.

### Symptom: `no task catalogs found under ...`

Diagnosis:

```sh
find . -name effigy.toml
```

Fix:
- run from correct workspace root, or
- create a manifest with `effigy init`.

### Symptom: `duplicate task catalog alias <...> found in ...`

Diagnosis:

```sh
effigy tasks
```

Fix:
- make aliases unique across all discovered manifests.

### Symptom: `failed to parse ...effigy.toml`

Diagnosis:

```sh
effigy doctor --verbose
```

Fix:
- correct TOML syntax and key names,
- remove unsupported keys (manifest uses strict unknown-field rejection).

### Symptom: `task <...> run step references unknown env entry <...>`

Diagnosis:

```sh
effigy tasks --resolve <task-selector>
```

Inspect the selected catalog `effigy.toml`:
- verify `[env]` contains the referenced entry name (`env = "<name>"`), or
- verify cross-catalog path refs (`env = "../shared/<name>"`) point to a real catalog root.

Fix:
- define the missing entry in `[env]`, or
- export it in process environment, or
- add it to `.env` / `env_file` fallback for that catalog.

### Symptom: `failed to read env file <...>`

Diagnosis:

```sh
effigy doctor --verbose
ls -la <catalog-root>/<env-file>
```

Fix:
- correct the `env_file` path (task-level or run-step),
- ensure file permissions allow read access,
- use an ordered fallback list (`env_file = [".env.local", ".env.test"]`) when files are optional per environment.

## 4) Built-in Test Routing Errors

### Symptom: `built-in test is ambiguous for arguments ...`

Diagnosis:

```sh
effigy test --plan <args>
```

Fix:

```sh
effigy test vitest <args>
effigy test nextest <args>
```

### Symptom: `built-in test runner <...> is not available ... Did you mean ...`

Diagnosis:

```sh
effigy test --plan <args>
```

Fix:
- use suggested suite name from error output,
- optionally define `[test.suites]` for explicit suite source-of-truth.

### Symptom: one or more built-in test targets failed

Diagnosis:

```sh
effigy test --verbose-results
```

Fix:
- inspect per-target command/exit diagnostics,
- rerun failing suite directly in that catalog root.

### Symptom: `CARGO_HOME` / `CARGO_TARGET_DIR` is missing during `effigy test`

Diagnosis:

```sh
effigy tasks --resolve test
effigy test --plan
```

Inspect:
- whether the planned command is cargo-shaped vs shell-wrapped
- whether the selected catalog defines `[env]` `CARGO_*` entries, or has fallback values in process env / `.env`

Fix:
- move custom cargo commands into named `[test.suites]` entries
- define `CARGO_*` values in the selected catalog `[env]` (highest precedence), or set `CARGO_HOME` / `CARGO_TARGET_DIR` in process env or `<target-root>/.env`
- by default (`cargo_env_match = "prefix-aware"`), use cargo command forms directly (for example `cargo test ...`, `env FOO=bar cargo test ...`) instead of `sh -lc "cargo ..."` wrappers
- if shell wrapping is required, set `[test].cargo_env_match = "shell-aware"`

### Symptom: built-in cargo suite command did not receive manifest `CARGO_*` env

Diagnosis:
- inspect command shape in `[test.suites]` and `effigy test --plan`

Fix:
- use a supported cargo executable form such as:

```sh
cargo test --workspace
cargo-nextest run --workspace
env RUST_LOG=info cargo test --workspace
/usr/local/bin/cargo nextest run --workspace
```

- when using default matching (`prefix-aware`), avoid shell-string wrappers where cargo is not the executable token (for example `sh -lc "cargo test --workspace"`)
- if shell wrappers are unavoidable, opt in with `[test].cargo_env_match = "shell-aware"`

### Symptom: lifecycle suite cleanup did not run after a failed test

Diagnosis:

```sh
effigy test --plan
```

Inspect:
- the selected `[test.suites.<name>]` table
- `teardown-steps`
- `teardown-policy`

Fix:
- set `teardown_policy = "always"` for suites that must clean up shared state even after setup or runner failure
- keep cleanup commands in `teardown`, not appended manually to the suite `run` command

### Symptom: runner filters or flags are being treated as Effigy arguments

Diagnosis:

```sh
effigy test --plan <args>
```

Fix:
- put the suite name first when needed, then use `--` before runner-specific arguments

Examples:

```sh
effigy test managed -- --package catalog_a-db --test learning_soft_delete
effigy test vitest -- --runInBand
effigy test nextest -- user_service --nocapture
```

### Symptom: setup or teardown commands cannot see suite env values

Diagnosis:

```sh
effigy test --plan
```

Inspect:
- `suite-env`
- `suite-env-files`
- whether the named env entry exists in `[env]`

Fix:
- define the missing env entry in `[env]`, or
- add the value to the selected dotenv file, or
- correct `env_file` on the suite table

## 5) Watch Mode and Lock Errors

### Symptom: ``--owner <effigy|external> is required``

Fix:

```sh
effigy watch --owner effigy --once test
```

### Symptom: ``--json requires a bounded watch run``

Fix:

```sh
effigy watch --owner effigy --once test --json
# or
effigy watch --owner effigy --max-runs 2 --json test
```

### Symptom: lock conflict for `task:watch:<target>`

Fix:

```sh
effigy tasks unlock task:watch:<target>
# if needed
effigy tasks unlock --all --yes
```

### Symptom: `lock conflict for <scope> ...`

Fix:

```sh
effigy tasks unlock shared:<name>
effigy tasks unlock task:<name>
effigy tasks unlock profile:<task>/<profile>
```

Use `--all` only when you cannot isolate a safe scope.

## 6) Managed TUI and Argument Errors

### Symptom: `managed task <...> profile <...> not found`

Diagnosis:

```sh
effigy tasks --task <task-name>
```

Fix:
- define `[tasks.<name>.profiles.<profile>]`, or
- run an existing profile.

### Symptom: `unknown argument` or `requires a value`

Diagnosis:

```sh
effigy <command> --help
```

Fix:
- correct flag shape and placement,
- for machine usage, prefer `effigy --json <command>`.

### Symptom: a headless managed task failed or appears stuck

Diagnosis:

```sh
effigy <task> status
effigy <task> logs
effigy <task> logs <process> --follow
```

The supervisor stores session state and numbered process logs under
`.effigy/runtime/managed/<task>-<profile>/`. A failed run reports the failing
process log path and tail. Readiness waits cover only container-owned routes
started by the lifecycle entry; `health_wait_timeout_secs` controls their
deadline.

Fix:
- inspect the named process log before restarting the whole stack
- correct the failing lifecycle, setup, or process command
- use `effigy <task> stop` to terminate a surviving supervisor cleanly
- use `EFFIGY_MANAGED_HEADLESS=1` when automation cannot pass `--headless`

## 7) Container and System Recovery

### Symptom: Docker Desktop is installed but Effigy still tries to use Colima, or vice versa

Diagnosis:

```sh
effigy doctor --verbose
```

The `Root Resolution` section shows:
- the backend Effigy selected
- whether a user-global backend/profile preference is pinned
- the active Docker context, when Docker is installed

Fix:
- pin a global preference with `effigy config set containers.backend docker` or
  `effigy config set containers.backend containerd`
- for one-shot override, pass `--backend docker` or `--backend containerd` to
  `bootstrap`
- clear a pinned preference with `effigy config unset containers.backend`

When the repo is already bootstrapped and you are trying to understand what it
will use next, also check:

```sh
effigy container status
```

That shows both:

- the manifest driver the repo declares
- the effective backend the current runtime is actually using

### Symptom: container environment is half-up or unresponsive

Diagnosis:

```sh
effigy container status
effigy system status
```

Fix:

```sh
# Gentle repair: restart services without data loss
effigy system repair

# Nuclear option: reset runtime state and rebuild from manifest
effigy system reset-runtime
```

Use `system repair` when:
- one service crashed but others are healthy
- you want to preserve container data and volumes
- the issue looks like a transient process failure

Use `system reset-runtime` when:
- compose state is corrupted or inconsistent
- multiple services are stuck in a bad state
- `repair` did not resolve the issue

### Symptom: doctor reports `container.workspace-ownership`

The running primary service contains root-owned paths in an Effigy-managed
workspace volume or `$BUN_INSTALL/install`, while workspace commands are
declared to run as another user.

Diagnosis:

```sh
effigy doctor --verbose
```

Fix:
- use the reported mount samples to locate the affected paths
- repair their ownership for the declared workspace user inside the workspace
  environment
- rerun `effigy doctor --verbose`

The finding is read-only. Host-routed workspace tasks and primary-service
`effigy exec` calls use the declared workspace user and HOME. Explicit
non-primary `--service` execs keep that service's configured user; pipes,
agents, and other non-console callers run without requesting a TTY.

### Symptom: caches or named volumes are piling up

Diagnosis:

```sh
effigy container cache list --global
effigy container volume list --dormant
```

Fix:

```sh
# drop only safe disposable build caches
effigy container cache prune --project <project-name> --yes

# drop repo-scoped stale named volumes the current repo no longer declares
effigy container volume prune --dormant --yes
```

Use `cache` for disposable artifacts such as `target`, `node_modules`,
`pnpm-store`, and Cargo caches. Use `volume` for repo-owned named volumes that
have gone stale as the repo evolved.

### Symptom: need to extract a bundled service for customization

```sh
# See what services are available
effigy service list

# Extract one service into your repo for modification
effigy service extract postgres --dir ./services
```

This writes the bundled service definition to `./services/postgres.toml` so you
can modify it without losing upstream updates.

### Symptom: need container resource usage or ejected compose files

```sh
# Show resource usage for running containers
effigy container stats

# Export the generated compose for manual inspection
effigy container <name> eject
```

Use `stats` when you need to debug memory/CPU issues.  
Use `eject` when you need to inspect or manually edit the generated compose configuration.

## 8) Deferral Failures

### Symptom: `deferral loop detected (...)`

Diagnosis:
- the unresolved request is bouncing through defer rules repeatedly.

Fix:
- tighten `[defer].run` conditions in the active manifest,
- avoid recursive calls that re-invoke the same unresolved selector chain.

## 8) When to Use `doctor` vs `tasks --resolve`

- use `effigy tasks --resolve <selector>` for routing evidence only,
- use `effigy doctor <selector> <args...>` for full explain output including selection and deferral reasoning.

## Expected Outcome

After this guide, you should be able to:

- choose the shortest useful diagnosis command for a failure
- separate routing issues from manifest, env, watch, and test issues
- turn a symptom into a concrete fix or a more precise follow-up question

## Related Guides

- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)

## Next Step

When symptom-level fixes are done, codify the preventative CI checks in [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md).
