# 021 - Quick Start and Command Cookbook

This guide is the shortest path from install to useful daily commands.

Use this page when you want the first ten minutes to feel clear. Use
[`025-command-reference-matrix.md`](./025-command-reference-matrix.md) when you
need the full command and flag surface.

## 1) Quick Start (5 Minutes)

Start with the CLI itself:

```sh
effigy --version
effigy --help
effigy init
effigy tasks
```

The default **`minimal`** `init` also drops a root **`README.md`** when that path
is empty; if you already have a project README there, Effigy **skips** it unless
you pass **`--force`**.

Use `effigy init --check --json` when you want the baseline managed setup
report. Use `effigy init --checklist --json` when you want the wider setup-job
inventory for agents or scripts. Plain `effigy init` prompts only on a real
TTY; otherwise it applies the missing deterministic baseline setup
idempotently.

When a caller wants explicit non-interactive setup execution:

```sh
effigy init --apply-actions manifest.effigy_toml,graph_status.inspect --json
```

`--version` shows the installed version (for example `v0.4.0`). Use it to confirm
you have a recent binary or when reporting issues.

If `effigy.toml` already exists, skip `init` and ask the repo what it knows
about:

```sh
effigy tasks
effigy tasks --resolve test
```

Then run something small and obvious:

```sh
effigy test
effigy doctor --verbose
```

`dev`, `build`, and similar names are **tasks your manifest defines** (unless
the guide names a built-in explicitly). They are not magic Effigy verbs.

If you are not sure which task will run, stop and use
`effigy tasks --resolve <task-name>` before guessing.

## 2) Minimal `effigy.toml`

```toml
[catalog]
alias = "app"

[tasks]
dev = "bun run dev"
"db:reset" = "./scripts/reset-db.sh"
build = "bun run build"
```

Run it with:

```sh
effigy tasks
effigy dev
effigy app/db:reset
effigy test --plan
```

**Core Concepts (30 seconds)**

- **Catalog** — a directory with an `effigy.toml`. The `alias` lets you prefix
tasks. `app/db:reset` means "run the `db:reset` task in the `app` catalog."
- **Task** — a named command defined in `effigy.toml`. Run it with `effigy <task>`.
- **Built-in** — commands like `test`, `watch`, and `init` that Effigy provides
automatically, even if they are not in your manifest.
- **Selector** — what you type after `effigy`: a task name, a catalog prefix
(`app/build`), or a built-in command.

Baseline mental model:

- define tasks in `effigy.toml`
- let Effigy discover nearby catalogs
- run tasks by intent instead of by directory or package manager
- keep `test` on the built-in orchestrator and configure custom routes as
  named `[test.suites]` entries

For fuller patterns such as multi-process dev stacks, systems, workspaces,
containers, demos, and manifest composition, continue to:

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)

## 3) Global JSON, `--repo`, and task-runtime prefix flags

### Global CLI flags

These apply to **top-level** commands (everything in `effigy --help`):

```sh
# Machine-readable JSON output (great for CI and scripts)
effigy --json tasks
effigy --json doctor
effigy --json test --plan

# Target a different repo without cd-ing into it
effigy --repo /path/to/other-project tasks
effigy --repo /path/to/other-project test

# Ask for help on any command
effigy --help
effigy test --help
effigy release --help
```

Use `--json` when another tool needs to parse the output.  
Use `--repo` when you want to run Effigy against a repo that is not your current directory.  
Use `--help` (or `-h`) when you need the exact flags for a specific command.

### Prefix flags on `effigy <task>` and `effigy <catalog>/<task>`

For **manifest-defined tasks**, Effigy parses these **before** task-specific
arguments (they must stay in the leading prefix segment, not after arbitrary
passthrough):

```sh
effigy --repo /path/to/other-project api/build
effigy serve --env-schema config/staging.env.schema
effigy validate --verbose-root
```

- **`--repo <PATH>`** — same meaning as the global form, but carried on a task
  invocation (parsed from the task argument list).
- **`--env-schema <PATH>`** — override which env schema file validates and
  merges plain (non-secret) values for that run. See
  [`050-env-schema-integration.md`](./050-env-schema-integration.md).
- **`--verbose-root`** — widen diagnostics and path resolution toward the
  repository root when the selected catalog is nested.

Built-ins that use Effigy's shared **passthrough** parser reject
`--verbose-root` and `--env-schema` on the **builtin** invocation itself
(today: `effigy doctor`, `effigy watch`, `effigy scan`). Use
`effigy <builtin> --help` for the exact flag set.

## 4) Optional: Local Dev Stacks With Containers

If the repo includes databases, caches, or language workspaces, use containers
to keep them off your host machine:

macOS prerequisites (Homebrew):

```sh
brew install colima
```

If Docker Desktop is also installed, Effigy can use either backend. Common
paths:

- keep Colima/containerd as the machine default:
  `effigy config set containers.backend containerd`
- switch the machine default to Docker Desktop:
  `effigy config set containers.backend docker`
- force one bootstrap session:
  `effigy bootstrap <git-url> --backend docker`

If the repo uses local HTTPS routes (`tls = true`), also install `mkcert` and
run the one-time trust-store install:

```sh
brew install mkcert
mkcert -install
```

```sh
effigy container up      # Start the local environment
effigy dev               # Run the repo's dev task inside it
```

When old local runtime state starts to pile up:

```sh
effigy container cache list --global
effigy container volume list --dormant
```

Read more: [`063-container-system-guide.md`](./063-container-system-guide.md)

## 5) Commands You Will Reach For First

### Discover what the repo can do

```sh
effigy tasks
effigy tasks --resolve test
effigy tasks --resolve app/build
```

Use these before running unfamiliar tasks.

### Check health and explain what Effigy sees

```sh
effigy doctor --verbose
effigy doctor --repo /path/to/workspace app/build --watch
```

Use the second form when you want explain-mode output for a specific task
and its passthrough args.

### Standardize tests and watch mode

```sh
effigy test --plan
effigy test vitest
effigy watch --owner effigy --once test
```

Use `--plan` first when you want to confirm what will run before running it.

### Start or migrate a manifest

```sh
effigy init
effigy tasks migrate --from package.json
effigy config --schema --minimal
```

Use these when the repo still depends on scattered scripts and ad-hoc setup.

### Bootstrap a repo from anywhere

```sh
effigy bootstrap git@github.com:inflatable-cookie/loophole.git --plan
effigy bootstrap git@github.com:inflatable-cookie/loophole.git
effigy bootstrap git@github.com:inflatable-cookie/loophole.git --start
```

Use this when the repo should describe its own bring-up path in `[bootstrap]`
instead of relying on a setup checklist or local shell history.

### Switch to automation-safe output

```sh
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

Use JSON mode when CI, scripts, or agents are consuming Effigy output.

### Build a bounded repo map before broad scanning

```sh
effigy graph index
effigy graph status --json
effigy graph explore "trace release orchestrator" --max-files 6 --max-bytes 12288 --json
git diff --name-only | effigy graph affected --stdin --json
effigy graph context "trace release orchestrator" --max-files 8 --max-bytes 4096 --json
```

Use this when an agent needs the first files to read without spraying `rg`
across the whole repo.

If `graph status --json` reports `stale_paths`, re-run `effigy graph index --json`
before trusting query results.

If the repo is changing while you work:

```sh
effigy graph watch --json
```

## 6) Manage Secrets

If the repo declares secrets under `[secrets.keys]`:

```sh
effigy secrets init              # create the vault
effigy secrets set database_url  # store a value
effigy secrets list              # inspect declarations
effigy secrets doctor            # check vault health
```

Secrets are injected into tasks, containers, Rhai scripts, and deploy hooks
without writing plaintext to repo files. See
[`075-secrets-and-vault-guide.md`](./075-secrets-and-vault-guide.md).

## 7) Choose the Next Detail Page

- Day-to-day workflows:
  [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- More manifest patterns:
  [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- Local containers, systems, and workspaces:
  [`063-container-system-guide.md`](./063-container-system-guide.md) and
  [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
- Task routing, tests, watch, or troubleshooting:
  [`016-task-routing-precedence.md`](./016-task-routing-precedence.md),
  [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md),
  [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- Demos, JSON, or bootstrap:
  [`058-demo-system-guide.md`](./058-demo-system-guide.md),
  [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md),
  [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md)

## Expected Outcome

After this guide, you should be able to:

- discover what a repo exposes through Effigy
- create or recognize a minimal `effigy.toml`
- run a task, inspect routing, and switch to JSON mode without guessing

## Related Guides

- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)
- [`012-dev-process-manager-tui.md`](./012-dev-process-manager-tui.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md)
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`075-secrets-and-vault-guide.md`](./075-secrets-and-vault-guide.md)
- [`073-state-stack-guide.md`](./073-state-stack-guide.md)
- [`074-deployment-guide.md`](./074-deployment-guide.md)

## Next Step

After this quick start, move to
[`055-everyday-workflows.md`](./055-everyday-workflows.md) to shape the daily
operator path, then use
[`022-manifest-cookbook.md`](./022-manifest-cookbook.md) to remove the next
piece of wrapper-script or directory-hunting friction from your repo.
