# Underlay starter

Reusable manifest shape for Underlay-style repos. Effigy resolves what you type
(**`effigy dev`**, **`effigy health`**, …) to **tasks** declared in your root
**`effigy.toml`** and the shipped **`underlay` bundle**, plus a small set of
**built-ins** (`test`, `init`, `doctor`, … — see **`effigy --help`**). A bundle
default **`dev`** is still a **task**, not a special CLI verb.

The stable
system/container layer comes from the shipped `underlay` bundle: one
long-running Rust + Bun workspace container, bundled postgres, dbgate,
mailpit, and minio services, managed gateway routes, and loopback alias
publication for `db.<host>`, `smtp.<host>`, and `s3.<host>`.

This shape uses only existing Effigy surfaces:

- `[bundle]` for the stable Underlay stack defaults
- bundle-owned managed `tasks.dev` with `role = "lifecycle"` + `role = "shell"`
- repo-owned tasks in the root manifest

There is no parallel "underlay runtime" concept in Effigy. The starter
is a convention built on the stable system/workspace/catalog model.

## Files

| File                       | Ownership       | Purpose                                                                                  |
|----------------------------|-----------------|------------------------------------------------------------------------------------------|
| `effigy.toml`              | consumer (root) | Declares `[bundle]`, optional `systems.dev.mounts`, repo alias, repo-owned tasks, and any explicit overrides. |

The default frontend hydration helper is a bundled asset referenced from
`effigy.toml` through `{{ bundle.root }}/scripts/dev/ui-setup.rhai`.
It is not emitted into the consumer repo. The helper reads `[bundle.dirs]`
when repos need explicit package-directory mapping instead of the default
`app-*` / `acme-*` guesses.

The shipped bundle also provides `smoke:error-logging`,
`metrics:error-log`, and `validate:error-reporting` through a bundled
`{{ bundle.root }}/scripts/error-reporting.rhai` helper. Override
`API_BASE_URL`, `SMOKE_ENDPOINT`, `WINDOW_HOURS`,
`NULL_RATE_THRESHOLD`, or `ERROR_REPORTING_ROUTES_DIR` when the repo's
API or route layout differs from the defaults.

The bundle-owned bootstrap run also uses the bundled
`{{ bundle.root }}/scripts/bootstrap-env.rhai` helper before container
startup. It creates app-local `.env` files only when they are missing,
deriving local URLs from `[bundle]` / `[bundle.routes]` and generating
local-only API secrets. Repos that intentionally fork that helper should
prefer Rhai's envfile-aware helpers such as `copy_if_missing(...)` and
`env_file_set(...)` for "seed then patch a few keys" flows instead of
reading and rewriting the entire file as raw text.

## Adoption

Run `effigy init underlay` inside the target repo — see
[`docs/guides/065-underlay-starter.md`](../../../../docs/guides/065-underlay-starter.md)
for the full emission contract, including `--dry-run` / `--force` / `--json`.

After emission, edit:

1. Update `[bundle]` in `effigy.toml`: `host`, `project_name`,
   `workspace_subdir`, `databases`, and the optional `api_port` /
   `admin_port` / `front_port` overrides.
   When the repo uses different app package names, also set
   `[bundle.dirs]` (`docs`, `api`, `client`, optional `ui`, `front`, `admin`). When the repo
   wants DNS labels to follow those app names, set `[bundle.routes]`
   (`front`, `admin`, `api`).
2. Add `systems.dev.mounts` in `effigy.toml` when sibling checkouts
   must be visible inside the workspace container.
3. The bundle owns the default root **`dev`** task and the standard **`health`**,
   **`validate`**, and **`qa`** aggregators. Add explicit root overrides only when
   the repo diverges from the usual docs / api / client / ui / front / admin layout.
4. Keep bundled setup helpers referenced through `{{ bundle.root }}`
   unless the repo intentionally needs to own a forked script.
5. Only add an explicit `[bootstrap]` block when the repo truly needs to
   override the bundle-owned default children or dependency sync behavior.

The same checklist is embedded in the starter's `starter.toml`
`[guidance]` block so `effigy init underlay` prints it on emit.

After adoption, the consumer repo should carry no `docker-compose.yml`
and no workspace Dockerfile. The root manifest only chooses bundle
inputs and repo-owned tasks; the stack shape and default bootstrap flow
stay in Effigy's bundled `underlay` defaults and service catalog.

## What stays consumer-owned

- Per-app `effigy.toml` in each child app (cargo/bun build glue, vite
  flags, migrations, jobs runner, etc.)
- Root `tasks.dev` only when the repo needs a non-standard concurrent
  shape
- Bundle inputs, sibling mounts, and any repo-specific port choices
- Custom setup scripts only when the bundled helper is not enough
