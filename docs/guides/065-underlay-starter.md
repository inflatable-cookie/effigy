# Shipped Bundles: Underlay and Decodelabs

This guide covers the two shipped top-level bundles that consumer repos can
adopt through `[bundle]` in `effigy.toml`:

- **`underlay`** — Rust + Bun workspace container + postgres + dbgate +
  mailpit + minio, intended for native apps that want one long-running workspace
  container plus an opinionated gateway route set. An `effigy init underlay`
  starter is shipped.
- **`decodelabs`** — PHP-native stack with php-fpm, nginx, MariaDB, phpMyAdmin,
  Memcached, and Redis, intended for Genesis-style web applications. No
  dedicated `effigy init` starter; adopt by writing `base = "decodelabs"`
  into `effigy.toml` directly.

Discover and inspect shipped bundles first:

```sh
effigy bundle list
effigy bundle inspect underlay
effigy bundle inspect decodelabs
```

If the shipped shape is close but needs repo-owned changes, export it and
switch the manifest to `base_path`:

```sh
effigy bundle export underlay --path bundles/underlay
```

The exported local bundle is not a lossy translation of the shipped one. Effigy
now uses the same canonical template source for shipped bundle defaults and for
`bundle export`, so local ownership starts from the exact same manifest shape.

## Underlay starter

Reusable Effigy manifest shape for Underlay-style consumer repos. The
stable system/container layer comes from the shipped `underlay` bundle:
one long-running Rust + Bun workspace container, bundled postgres, dbgate,
mailpit, and minio services, a gateway-fronted domain set, and loopback
alias publication for `db.<host>`, `smtp.<host>`, and `s3.<host>`.

Use this when a repo wants the Underlay local-dev shape without copying
`docker-compose.yml`, `workspace.Dockerfile`, and large system/container
overrides from an existing Underlay repo.

## Emit the starter

```
effigy init underlay
```

Emits one root manifest into the current repo:

| File                        | Purpose                                                                                      |
|-----------------------------|----------------------------------------------------------------------------------------------|
| `effigy.toml`               | Root manifest. `[bundle]`, optional `systems.dev.mounts`, repo alias, repo-owned tasks, and any explicit overrides. |

The default UI setup script is a bundled asset referenced from
`effigy.toml` through `{{ bundle.root }}/scripts/dev/ui-setup.rhai`.
It is not copied into the consumer repo. The helper reads `[bundle.dirs]`
when repos need explicit package-directory mapping instead of the default
`app-*` / `acme-*` guesses. If a repo still needs custom hydration after
that, point the setup step at a repo-owned script instead.

The bundle also publishes error-reporting helper tasks that run a bundled
Rhai script from `{{ bundle.root }}/scripts/error-reporting.rhai`:

- `smoke:error-logging` posts to `https://api.<host>/v1/dev/error-smoke`
  by default, then verifies the latest `platform.error_log` row.
- `metrics:error-log` reports `handler_context` null-rate metrics for
  recent rows.
- `validate:error-reporting` combines route-pattern checks, the smoke
  probe, and metrics.

Set `API_BASE_URL`, `SMOKE_ENDPOINT`, `WINDOW_HOURS`,
`NULL_RATE_THRESHOLD`, or `ERROR_REPORTING_ROUTES_DIR` when a repo needs
different defaults.

Supported flags mirror the rest of `effigy init`:

- `--dry-run` — print each file's content under a `=== <target> ===`
  header without touching disk.
- `--force` — overwrite existing targets. Without `--force`, `init`
  refuses to clobber any target that already exists and lists every
  conflicting path.
- `--json` — emit the `effigy.init.v1` contract (see
  [guide 017](./017-json-output-contracts.md)) with a per-file
  `files[]` array and a top-level `guidance` string.

After emission the text output prints the post-emission guidance block
embedded in the starter descriptor (edit checklist below).

## Post-emission edit checklist

The `effigy init underlay` output already prints this list; keeping a
copy here so users can refer back without re-emitting.

1. In `effigy.toml`:
   - set `[bundle].host` to the project's front-end domain
   - rename `[bundle].project_name`
   - set `[bundle].workspace_subdir` to the repo's directory name under `/workspace-root`
   - set `[bundle].databases = ["app"]` to the repo's dev database name
   - when the repo needs more than one database, extend that list:
     `[bundle].databases = ["app", "app_test", ...]`. The first entry
     becomes the primary database and all entries are created at first boot.
   - align `[bundle].api_port`, `[bundle].admin_port`, and `[bundle].front_port` if the repo uses different dev-server ports
   - when the repo uses different app package names, set `[bundle.dirs]` (`docs`, `api`, `client`, optional `ui`, `front`, `admin`)
   - when gateway labels should follow those app names too, set `[bundle.routes]` (`front`, `admin`, `api`)
   - optionally override the name knobs — `[bundle].system_name`
     (default `dev`), `[bundle].container_name` (default `stack`),
     `[bundle].workspace_service_name` (default `workspace`), and
     `[bundle].default_workspace` (default `app`) — when the defaults
     collide with existing repo conventions. Keep the root `tasks.*`
     entries and any `systems.<name>` override blocks aligned with the
     new names.
   - adjust `systems.dev.mounts` for any sibling checkouts

2. In `effigy.toml` tasks:
   - the bundle owns the default root `dev`, `health`, `validate`, and
     `qa` tasks
   - only add explicit root overrides when the repo really diverges
     from the standard docs/api/client/ui/front/admin shape
   - keep bundled setup helpers referenced through `{{ bundle.root }}`
     unless the repo intentionally needs to own a forked script

3. Only add an explicit `[bootstrap]` block when the repo truly needs to
   override the bundle-owned default children or dependency sync behavior.

After the edit pass, the consumer repo carries **no** `docker-compose.yml`
and **no** workspace Dockerfile. The root manifest only chooses bundle
inputs and repo-owned tasks; the stack shape is generated from Effigy's
bundled defaults each run.

## Bundle app mapping and route labels

`[bundle.dirs]` tells the bundled `ui-setup.rhai` helper which packages
back the docs lane, shared API client, optional UI package, and the
front/admin surfaces:

```toml
[bundle.dirs]
docs = "packages/docs"
client = "packages/api-client"
ui = "packages/ui"
front = "packages/web"
admin = "packages/dashboard"
```

When unset, the helper guesses against `app-*` and `acme-*` package
names. Set `[bundle.dirs]` explicitly when the repo's package names do
not follow either convention.

`[bundle.routes]` labels the gateway routes the bundle registers for
each app surface. Defaults follow the `app-*` guesses; override when the
gateway should expose more meaningful names:

```toml
[bundle.routes]
front = "app"
admin = "dashboard"
api = "api"
```

The labels show up in the managed dev task's lifecycle tab and in
gateway DNS routes (`<label>.<host>`). Both tables are underlay-bundle
inputs and have no effect on the decodelabs bundle.

## What the starter is built on

The starter uses only the stable Effigy model:

- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
  for `systems` / `workspaces` / managed `dev`.
- [`063-container-system-guide.md`](./063-container-system-guide.md) for
  the generated service catalog and `[containers.<name>.services.*]`.
- manifest bundles for the stable Underlay stack preset.

No new runtime concept is introduced.

## The `workspace-rust-bun` bundled service

The starter's `workspace` service comes from the `underlay` bundle and
resolves to a bundled catalog fragment named **`workspace-rust-bun`**.
It ships:

- `rust:${RUST_VERSION}-bookworm` base image
- Bun (latest by default, pinnable via `bun_version`)
- a non-root `dev` user aligned with host UID/GID so bind-mounted files
  round-trip cleanly
- `command: sleep infinity` — the container runs as a shell target and
  command runner, not a service
- persistent named volumes for cargo registry + cargo git
- bundle-provided dev-server port bindings for the three default app
  routes (`api_port`, `admin_port`, `front_port`)
- a healthcheck that verifies cargo and bun are on `PATH`

The same bundle also declares the bundled `postgres`, `dbgate`, `mailpit`,
and `minio` services plus the default gateway routes:

- `https://<host>`
- `https://admin.<host>`
- `https://api.<host>`
- `https://dbgate.<host>`
- `http://mailpit.<host>`
- `http://minio.<host>`

At runtime the current gateway surface also derives project-owned
loopback aliases for:

- `db.<host>:5432`
- `smtp.<host>:1025`
- `s3.<host>:9000`

Parameters (see `service.toml` in the fragment directory for full
details):

| Param            | Default             | Purpose                                                            |
|------------------|---------------------|--------------------------------------------------------------------|
| `rust_version`   | `"1.88"`            | Rust base image tag.                                               |
| `bun_version`    | `""` (latest)       | Pin a specific Bun release.                                        |
| `workspace_mount`| `"/workspace-root"` | Mount point for the workspace root inside the container.           |
| `working_subdir` | `""` (= mount root) | Subdirectory of `workspace_mount` to set as the compose `working_dir`. |
| `host_ports`     | `[]`                | Catalog-level escape hatch for explicit workspace port bindings. The shipped `underlay` bundle fills this from `api_port`, `admin_port`, and `front_port`. |

System-layer overrides still apply on top:

- `systems.<name>.user` and `systems.<name>.working_dir` win at task
  time via `docker compose exec -u <user> -w <working_dir>` — the
  Dockerfile's `USER`/`WORKDIR` are defaults only.
- `systems.<name>.mounts` are injected into the workspace service's
  compose `volumes` at runtime.
- mounted sibling repos in `systems.<name>.mounts` automatically adopt any
  producer-declared `[isolation].paths` into the workspace container; the
  normal underlay-consumer path does not need a second sibling list under
  `systems.<name>.isolation`.

## What stays consumer-owned

- Per-app `effigy.toml` in each child app (cargo/bun build commands,
  vite flags, migrations, jobs runner, etc.)
- Root `tasks.dev` only when the repo needs a non-standard concurrent shape
- Project domains, project name, and app dev-server ports through
  `[bundle]` inputs
- Sibling-checkout layout in `systems.dev.mounts`
- Custom setup scripts only when the bundled helper is not enough
- Custom error-reporting scripts only when the bundled helper does not fit
  the repo's API/error-log shape

## Proof

The integration suite covers both the fragment and the starter:

- `crates/effigy-catalog/tests/integration.rs`
  - `resolve_workspace_rust_bun_fragment`
  - `workspace_rust_bun_assembles_with_defaults`
  - `workspace_rust_bun_publishes_host_ports_when_requested`
  - `underlay_style_stack_assembles_with_bundled_fragments_only`
- `crates/effigy-catalog/src/starter.rs`
  - `underlay_starter_resolves_with_all_declared_files_and_guidance`
- `crates/effigy-manifest/tests/underlay_starter.rs`
  - verifies the starter composes into a single manifest via
    `[manifest].include`
  - verifies the root manifest resolves the shipped `underlay` bundle
  - verifies `systems.dev` binds the `stack` container
  - verifies the four expected services resolve to their bundled
    catalog fragments
  - verifies `tasks.dev` wires both `role = "lifecycle"` and
    `role = "shell" service = "workspace"` runtime-contract entries
  - verifies the bootstrap config and aggregator tasks are present
- `src/tests/runner_tests/runner_core_tests/init_migrate_tests/init_tests.rs`
  - `run_manifest_task_builtin_init_underlay_emits_all_declared_files_and_guidance`
  - `run_manifest_task_builtin_init_underlay_refuses_overwrite_without_force`
  - `run_manifest_task_builtin_init_underlay_force_overwrites_all_targets`
  - `run_manifest_task_builtin_init_underlay_dry_run_prints_fenced_sections_without_writing`
  - `run_manifest_task_builtin_init_underlay_json_reports_files_array_and_guidance`

## Decodelabs bundle

Reusable Effigy manifest shape for PHP-native (Genesis-style) consumer repos.

### Adopt by editing `effigy.toml`

`decodelabs` has no dedicated `effigy init` starter. Adopt the bundle by
declaring it directly in the root manifest:

```toml
[bundle]
base = "decodelabs"
host = "example.legacy.test"
project_name = "my-project-dev"
databases = ["my_database"]
```

| Input                    | Default     | Purpose                                                                      |
|--------------------------|-------------|------------------------------------------------------------------------------|
| `host`                   | _required_  | Primary local hostname. Gateway registers the `web` service on `<host>` and `pma.<host>`. |
| `project_name`           | _required_  | Docker Compose project name for the generated stack.                          |
| `databases`              | _required_  | MariaDB databases to create for the stack. Use `["app"]` for the normal single-db case; the first entry is also wired into the `mysql` workspace alias. |
| `zest_port`              | optional    | Publish and gateway-route a temporary Zest/Vite dev server running inside the `app` workspace service. Use the same fixed port as the site's `vite.config.ts`. |
| `zest_domain`            | optional    | Override the Zest/Vite route hostname. Defaults to `zest.<host>` when `zest_port` is set. |
| `system_name`            | `"dev"`     | Name of the `[systems.<name>]` block rendered by the bundle.                  |
| `container_name`         | `"web"`     | Name of the `[containers.<name>]` block and the default container.            |
| `workspace_service_name` | `"app"`     | Name of the php-fpm service (also the `php` alias target and the `composer` service). |
| `default_workspace`      | `"app"`     | `[systems.<system>.workspaces.<name>]` treated as the system default.         |

### Services composed by the bundle

| Service    | Image                 | Purpose                                                                                     |
|------------|-----------------------|---------------------------------------------------------------------------------------------|
| `app`      | `php:8.4-fpm`         | PHP-FPM workspace with Composer, Node 20, and extensions (`pdo_mysql`, `intl`, `exif`, `zip`, `gd`, `redis`, `memcached`, `opcache`). Ships `decodelabs/effigy` globally. |
| `web`      | `nginx`               | Nginx in front of `app` using the bundled `decodelabs` config variant. Document root `.`, rewrites every request to `/vendor/genesis.php`, and hands off to php-fpm. No `try_files`, asset caching, or security locations — DecodeLabs apps handle routing, asset serving, and error pages in PHP. See guide 067 for the variant reference. |
| `db`       | `mariadb:10.11`       | MariaDB with the configured `database` created on first start.                              |
| `pma`      | `phpmyadmin:latest`   | phpMyAdmin connected to `db`.                                                               |
| `memcache` | `memcached`           | In-memory cache sized at 128 MB by default.                                                 |
| `redis`    | `redis:7`             | Key-value store.                                                                            |

### Gateway routes registered by default

- `https://<host>` -> `web`
- `https://pma.<host>` -> `pma`
- `https://zest.<host>` -> `app:<zest_port>` when `zest_port` is set

TLS is enabled by default through `effigy gateway setup-tls`.
The temporary Zest/Vite process still needs to bind `0.0.0.0` on that same
fixed port for the route to answer from the host.

### Bundled tasks

The bundle ships one ready-to-run task on top of the standard managed
`dev` task:

- `tasks.seed` — runs the bundled
  `{{ bundle.root }}/scripts/seed-latest-db-dump.rhai` script, which
  imports the newest `.sql` dump from `.effigy/local/db-seeds/` into the
  primary database. Drop a dump named `<database>-<timestamp>.sql` into
  that directory, then run `effigy run seed`.

To override a bundled script, ship a same-named file under the repo's
own `scripts/` directory and point the task at the local path instead of
`{{ bundle.root }}/...`.

### Workspace aliases

- `php` — run `php` inside the `app` service
- `composer` — run Composer inside the `app` service
- `mysql` — run `mysql` inside the `db` service with credentials pre-populated

### Default system defaults

- default system: `dev` (configurable via `[bundle].system_name`)
- default container: `web` (configurable via `[bundle].container_name`)
- primary service: `app` (configurable via `[bundle].workspace_service_name`)
- default workspace: `app` (configurable via `[bundle].default_workspace`)
- working directory: `/var/www/html`
- startup mode: detached
- lifecycle: `on_task_exit = "stop"`, graceful shutdown

### Adoption checklist

1. Write the `[bundle]` block above into the repo's `effigy.toml`.
2. Run `effigy bundle inspect decodelabs` to confirm the bundle resolved
   with the expected inputs.
3. Run `effigy container up` (or `effigy system up` if the manifest declares
   `[systems.<name>]`) to bring the stack online.
4. Use `effigy workspace` or `effigy exec` for app-level work inside the
   `app` service.
5. Point DNS at the gateway with `effigy gateway up` and, when appropriate,
   `effigy gateway setup-tls` so `https://<host>` resolves cleanly.

### Proof

Decodelabs bundle coverage lives alongside the underlay proofs under
`crates/effigy-catalog/` (bundle resolution, service composition, gateway
route derivation). Consumer repos on the Decodelabs stack consume this
surface directly via `base = "decodelabs"`.
