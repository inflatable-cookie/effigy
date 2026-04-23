# Underlay starter

Reusable manifest shape for Underlay-style repos. The stable
system/container layer comes from the shipped `underlay` bundle: one
long-running Rust + Bun workspace container, bundled postgres, dbgate,
mailpit, and minio services, managed gateway routes, and loopback alias
publication for `db.<host>`, `smtp.<host>`, and `s3.<host>`.

This shape uses only existing Effigy surfaces:

- `[bundle]` for the stable Underlay stack defaults
- managed `tasks.dev` with `role = "lifecycle"` + `role = "shell"`
- `[manifest].include` composition

There is no parallel "underlay runtime" concept in Effigy. The starter
is a convention built on the stable system/workspace/catalog model.

## Files

| File                       | Ownership       | Purpose                                                                                  |
|----------------------------|-----------------|------------------------------------------------------------------------------------------|
| `effigy.toml`              | consumer (root) | Declares `[bundle]`, optional `systems.dev.mounts`, repo alias, and `[manifest].include`. |
| `effigy.tasks.toml`        | starter         | Managed `tasks.dev` concurrent shape, plus `health`/`validate`/`qa` aggregators.         |
| `effigy.bootstrap.toml`    | starter         | `bootstrap:deps` and the `[bootstrap]` entry points.                                     |

The default frontend hydration helper is a bundled asset referenced from
`effigy.tasks.toml` through `{{ bundle.root }}/scripts/dev/ui-setup.rhai`.
It is not emitted into the consumer repo.

The shipped bundle also provides `smoke:error-logging`,
`metrics:error-log`, and `validate:error-reporting` through a bundled
`{{ bundle.root }}/scripts/error-reporting.rhai` helper. Override
`API_BASE_URL`, `SMOKE_ENDPOINT`, `WINDOW_HOURS`,
`NULL_RATE_THRESHOLD`, or `ERROR_REPORTING_ROUTES_DIR` when the repo's
API or route layout differs from the defaults.

## Adoption

Run `effigy init underlay` inside the target repo — see
[`docs/guides/065-underlay-starter.md`](../../../../docs/guides/065-underlay-starter.md)
for the full emission contract, including `--dry-run` / `--force` / `--json`.

After emission, edit:

1. Update `[bundle]` in `effigy.toml`: `host`, `project_name`,
   `workspace_subdir`, `database`, and the optional `api_port` /
   `admin_port` / `front_port` overrides.
2. Add `systems.dev.mounts` in `effigy.toml` when sibling checkouts
   must be visible inside the workspace container.
3. Replace the `app-*/dev` concurrent entries in `effigy.tasks.toml`
   and the aggregator task lists with the repo's real apps.
4. Edit the `bootstrap:deps` run command in `effigy.bootstrap.toml` to
   match the repo's dependency fetch sequence.
5. Keep bundled setup helpers referenced through `{{ bundle.root }}`
   unless the repo intentionally needs to own a forked script.

The same checklist is embedded in the starter's `starter.toml`
`[guidance]` block so `effigy init underlay` prints it on emit.

After adoption, the consumer repo should carry no `docker-compose.yml`
and no workspace Dockerfile. The root manifest only chooses bundle
inputs and repo-owned tasks; the stack shape itself stays in Effigy's
bundled `underlay` defaults and service catalog.

## What stays consumer-owned

- Per-app `effigy.toml` in each child app (cargo/bun build glue, vite
  flags, migrations, jobs runner, etc.)
- Tab ordering and concurrent makeup in `tasks.dev`
- Bundle inputs, sibling mounts, and any repo-specific port choices
- Custom setup scripts only when the bundled helper is not enough
