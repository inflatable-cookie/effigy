# 021 - Quick Start and Command Cookbook

This guide is the shortest path from install to useful daily commands.

Use this page when you want the first ten minutes to feel clear. Use
[`025-command-reference-matrix.md`](./025-command-reference-matrix.md) when you
need the full command and flag surface.

## 1) Quick Start (5 Minutes)

Start with the CLI itself:

```sh
effigy --help
effigy init
effigy tasks
```

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

If routing or task ownership is not obvious yet, stop and use
`effigy tasks --resolve <selector>` before guessing.

If the repo is web- or service-heavy, the first useful local-dev path is now:

```sh
effigy service list
effigy container up
effigy gateway status
effigy exec <command>
effigy dev
```

Use those in order when the repo has bundled service fragments, a managed
container environment, local domains, and one repo-owned `dev` front door.

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

Baseline mental model:

- define tasks in `effigy.toml`
- let Effigy discover nearby catalogs
- run tasks by intent instead of by directory or package manager
- leave `test` to the built-in runner unless you intentionally want explicit
  `tasks.test` behavior

For fuller patterns such as multi-process dev stacks, systems, workspaces,
containers, demos, and manifest composition, continue to:

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)

## 3) Commands You Will Reach For First

### Discover and route work

```sh
effigy tasks
effigy tasks --resolve test
effigy tasks --resolve app/build
```

Use these before running unfamiliar selectors.

### Check health and explain what Effigy sees

```sh
effigy doctor --verbose
effigy doctor --repo /path/to/workspace app/build -- --watch
```

Use the second form when you want explain-mode output for a specific selector
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
effigy migrate --from package.json
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

## 4) Choose the Next Detail Page

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

## Next Step

After this quick start, move to
[`055-everyday-workflows.md`](./055-everyday-workflows.md) to shape the daily
operator path, then use
[`022-manifest-cookbook.md`](./022-manifest-cookbook.md) to remove the next
piece of wrapper-script or directory-hunting friction from your repo.
