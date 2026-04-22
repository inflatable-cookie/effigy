# Underlay starter

Reusable manifest shape for Underlay-style repos — a long-running
Rust + Bun workspace container, bundled dev services (postgres, mailpit,
minio), and a managed `dev` TUI task.

This shape uses only existing Effigy surfaces:

- `[systems]` / `[systems.dev]` / `[systems.dev.workspaces.app]`
- `[containers.stack]` with `[containers.stack.services.<name>]` entries
  resolved against the bundled catalog
- managed `tasks.dev` with `role = "lifecycle"` + `role = "shell"`
- `[manifest].include` composition

There is no parallel "underlay runtime" concept in Effigy. The starter
is a convention built on the stable system/workspace/catalog model.

## Files

| File                       | Ownership       | Purpose                                                                                  |
|----------------------------|-----------------|------------------------------------------------------------------------------------------|
| `effigy.toml`              | consumer (root) | Pulls the starter fragments in via `[manifest].include`. Hosts the catalog alias.        |
| `effigy.system.toml`       | starter         | `systems.dev` + `containers.stack` + bundled service declarations + gateway DNS routes.  |
| `effigy.tasks.toml`        | starter         | Managed `tasks.dev` concurrent shape, plus `health`/`validate`/`qa` aggregators.         |
| `effigy.bootstrap.toml`    | starter         | `bootstrap:deps` and the `[bootstrap]` entry points.                                     |
| `scripts/dev/ui-setup.rhai`| starter         | Frontend hydration helper invoked from `tasks.dev` concurrent entries as a `setup` step. |

## Adoption

Run `effigy init underlay` inside the target repo — see
[`docs/guides/065-underlay-starter.md`](../../../../docs/guides/065-underlay-starter.md)
for the full emission contract, including `--dry-run` / `--force` / `--json`.

After emission, edit:

1. `systems.dev.working_dir` in `effigy.system.toml` to match the
   repo's position inside the workspace root — typically
   `/workspace-root/<repo-name>`.
2. `containers.stack.project_name` and the DNS domains to match
   the consumer's naming.
3. Replace the `app-*/dev` concurrent entries in `effigy.tasks.toml`
   and the aggregator task lists with the repo's real apps.
4. Edit the `bootstrap:deps` run command in `effigy.bootstrap.toml` to
   match the repo's dependency fetch sequence.
5. Edit `scripts/dev/ui-setup.rhai` to hydrate the repo's real frontend
   packages. Sibling-repo hydration is left commented out in the
   default; re-enable it if the repo relies on sibling checkouts.

The same checklist is embedded in the starter's `starter.toml`
`[guidance]` block so `effigy init underlay` prints it on emit.

After adoption, the consumer repo should carry no `docker-compose.yml`
and no workspace Dockerfile — both are generated from the bundled
`workspace-rust-bun` / `postgres` / `mailpit` / `minio` catalog
fragments.

## What stays consumer-owned

- Per-app `effigy.toml` in each child app (cargo/bun build glue, PORT
  and env strings, vite flags, migrations, jobs runner, etc.)
- Tab ordering and concurrent makeup in `tasks.dev`
- Project domains, project name, and port numbers
- The `ui-setup.rhai` helper (starter-seeded, consumer-owned thereafter)
