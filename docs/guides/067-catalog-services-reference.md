# 067 - Catalog Services Reference

Use this guide when you want to know exactly which shipped service fragments
Effigy can compose into a `[systems.<name>]` or `[containers.<name>]`
manifest, what each one accepts, and what it exposes by default.

These fragments are what make `services.<svc> catalog = "<name>"` work. Each
one is a small pinned compose fragment with a narrow parameter surface and
explicit defaults.

If you want to author or change a catalog service, use
[`071-catalog-service-authoring.md`](./071-catalog-service-authoring.md).

## How To Use A Catalog Service

Catalog-driven form in `effigy.toml`:

```toml
[containers.web.services.db]
catalog = "postgres"
version = "16"
database = "my_app"

[containers.web.services.cache]
catalog = "redis"
```

Or under a system:

```toml
[systems.dev.services.db]
catalog = "postgres"

[systems.dev.services.cache]
catalog = "redis"
```

Inspect the shipped bundle surface directly:

```sh
effigy bundle list
effigy bundle inspect underlay
effigy bundle inspect decodelabs
```

Catalog-driven services land in generated compose under
`.effigy/runtime/compose/`; see
[`063-container-system-guide.md`](./063-container-system-guide.md) for the
runtime layout.

## Catalog Services

Each section below covers one shipped fragment. Parameters shown with
`default` are the current built-in defaults; override them inline under the
service definition.

### `workspace-rust-bun`

Long-running Rust + Bun workspace container. Used by `[bundle].base =
"underlay"` as the default workspace service and by managed tasks that open
`role = "shell" service = "workspace"`.

- Image: custom Dockerfile (Rust + Bun base).
- Parameters:
  - `rust_version` (default `"1.88"`) — Rust base image tag.
  - `bun_version` (default `""` = latest) — pinnable Bun release.
  - `workspace_mount` (default `"/workspace-root"`) — in-container mount
    point for the workspace root.
  - `working_subdir` (default `""`) — subdirectory of `workspace_mount` used
    as the container's `working_dir`.
  - `host_ports` (default `[]`) — explicit host port bindings (for example
    filled in by the `underlay` bundle from `api_port`/`admin_port`/
    `front_port`).
- Exposed ports: driven by `host_ports`.
- Volumes: `cargo-registry` at `/usr/local/cargo/registry` and `cargo-git` at
  `/usr/local/cargo/git`, both named and persistent.
- Healthcheck: `command -v cargo && command -v bun`.
- Shell target: yes (`/bin/bash`); usable for `effigy workspace` and
  `role = "shell"` tabs.

### `postgres`

PostgreSQL database server.

- Image: `postgres:16-alpine` (override via `version`).
- Parameters: `version` (`"16"`), `database` (`"app"`), `databases`
  (string array, optional), `password` (`"secret"`).
- Exposed port: `5432`.
- Volume: persistent data volume at `/var/lib/postgresql/data`.
- Healthcheck: `pg_isready -U postgres`.
- Shell target: yes (`/bin/bash`).
- Gateway: eligible for TCP DNS alias + loopback IP allocation when the
  containing environment declares `[containers.<name>.dns]`.
- Multi-database mode: set `databases = ["app", "app_test", "reporting"]`
  to create more than one database at startup. The first entry becomes the
  primary app database (compatible with `database`); all entries are
  created on first boot. Singular `database` keeps working for single-db
  stacks.

### `dbgate`

Modern SQL/NoSQL database manager with a spreadsheet-style data grid,
row editing, foreign-key lookups, form view for wide tables, and a SQL
editor with history. This is the default database UI in the shipped
`underlay` bundle.

- Image: `dbgate/dbgate:latest` (override via `version`).
- Parameters: `version` (`"latest"`), `database_host` (`"postgres"`),
  `database_port` (`5432`), `database` (`"postgres"`),
  `database_user` (`"postgres"`), `database_password` (`""`),
  `engine` (`"postgres@dbgate-plugin-postgres"`),
  `connection_label` (`"Postgres"`).
- Exposed port: `3000`.
- Volume: persistent `data` volume at `/root/.dbgate` for saved queries
  and settings.
- Healthcheck: HTTP `GET /` on port 3000.
- Shell target: no.
- Depends on: optional `postgres` or `mariadb` service in the same
  environment. Pulls the target `database` / `password` from the linked
  service's params when those inputs are left unset.
- Engine override: set `engine = "mysql@dbgate-plugin-mysql"` (or
  `"mariadb@dbgate-plugin-mysql"`) to front MariaDB/MySQL instead.
- Gateway: typically exposed as `dbgate.<host>` via
  `[containers.<name>.dns]`.

### `pgweb`

Lightweight browser UI for local PostgreSQL access. Read-oriented — no
row editor, write operations happen through the built-in SQL query tab.
Kept in the catalog as a lean alternative to `dbgate` when the heavier
UI is not needed.

- Image: `sosedoff/pgweb:latest` (override via `version`).
- Parameters: `version` (`"latest"`), `database_host` (`"postgres"`),
  `database_port` (`5432`), `database` (`"postgres"`),
  `database_user` (`"postgres"`), `database_password` (`""`).
- Exposed port: `8081`.
- Volumes: none.
- Healthcheck: HTTP `GET /` on port 8081.
- Shell target: no.
- Depends on: optional `postgres` service in the same environment.
- Gateway: typically exposed as `pgweb.<host>` via
  `[containers.<name>.dns]`.

### `mariadb`

MariaDB database server.

- Image: `mariadb:10.11` (override via `version`).
- Parameters: `version` (`"10.11"`), `database` (`"app"`), `databases`
  (string array, optional), `password` (`"secret"`).
- Exposed port: `3306`.
- Volume: persistent data volume at `/var/lib/mysql`.
- Healthcheck: `healthcheck.sh --connect --innodb_initialized`.
- Shell target: yes (`/bin/bash`).
- Gateway: eligible for TCP DNS alias + loopback IP allocation.
- Multi-database mode: set `databases = ["app", "app_test"]` to create
  more than one database at startup. The first entry becomes the primary
  app database (compatible with `database`); all entries are created on
  first boot.

### `redis`

Redis key-value store.

- Image: `redis:7-alpine` (override via `version`).
- Parameters: `version` (`"7"`).
- Exposed port: `6379`.
- Volumes: none (in-memory).
- Healthcheck: none built in.
- Shell target: yes (`/bin/sh`).
- Gateway: eligible for TCP DNS alias + loopback IP allocation.

### `memcached`

Memcached in-memory cache.

- Image: `memcached:1.6-alpine` (override via `version`).
- Parameters: `version` (`"1.6"`), `memory` (`64` MB).
- Exposed port: `11211`.
- Volumes: none.
- Healthcheck: none built in.
- Shell target: no.
- Gateway: eligible for TCP DNS alias + loopback IP allocation.

### `mailpit`

Local SMTP catcher and web UI for development email.

- Image: `axllent/mailpit:latest` (override via `version`).
- Parameters: `version` (`"latest"`), `smtp_port` (`1025`), `ui_port`
  (`8025`).
- Exposed ports: SMTP on `1025`, web UI on `8025`.
- Volumes: none.
- Healthcheck: HTTP `GET /` on the UI port.
- Shell target: no.
- Gateway: eligible for HTTP route (for example `mailpit.<host>`) through
  `[containers.<name>.dns]`; also gets a TCP loopback alias for the SMTP
  port when the bundle wires one.

### `minio`

Local S3-compatible object storage and console.

- Image: `minio/minio:latest` (override via `version`).
- Parameters: `version` (`"latest"`), `root_user` (`"minioadmin"`),
  `root_password` (`"minioadmin"`), `api_port` (`9000`), `console_port`
  (`9001`).
- Exposed ports: S3 API on `9000`, console on `9001`.
- Volume: persistent `data` volume at `/data`.
- Healthcheck: `mc ready local`.
- Shell target: yes (`/bin/sh`).
- Gateway: eligible for HTTP route plus TCP loopback alias for the S3 port.

### `elasticsearch`

Elasticsearch search engine.

- Image: `elasticsearch:8.15.0` (override via `version`).
- Parameters: `version` (`"8.15.0"`), `java_opts` (`"-Xms512m -Xmx512m"`).
- Exposed port: `9200` (HTTP API).
- Volume: persistent `data` volume at `/usr/share/elasticsearch/data`.
- Healthcheck: `curl -sf http://127.0.0.1:9200/_cluster/health`.
- Shell target: yes (`/bin/bash`).
- Gateway: eligible for HTTP route + TCP loopback alias.

### `phpmyadmin`

Web UI for MariaDB/MySQL.

- Image: `phpmyadmin:latest` (override via `version`).
- Parameters: `version` (`"latest"`), `database_host` (`"db"`),
  `database_port` (`3306`), `database_password` (`""`).
- Exposed port: `80`.
- Volumes: none.
- Healthcheck: HTTP `GET /` on port 80.
- Shell target: no.
- Depends on: optional `mariadb` or `postgres` service in the same
  environment.
- Gateway: typically exposed as `pma.<host>` via `[containers.<name>.dns]`.

### `nginx`

Nginx reverse proxy / static web server for PHP-FPM setups.

- Image: `nginx:alpine`.
- Parameters:
  - `document_root` (default `"public"`)
  - `working_dir` (default `"/var/www/html"`)
  - `front_controller_fallback` (default `"/index.php?$query_string"`)
  - `asset_fallback` (default `"/index.php?$query_string"`)
  - `rewrite_all_to` (default `""`)
  - `error_page_404` (default `"/index.php"`)
- Exposed port: `80`.
- Volumes: repo root (read-only bind) plus service config (read-only bind).
- Healthcheck: HTTP `GET /` on port 80.
- Shell target: no.
- Depends on: optional `php-fpm` service in the same environment.
- Gateway: typically the primary HTTP route (`<host>`) via
  `[containers.<name>.dns]`.

Config variants (selected via `variant = "<name>"` on the service or via
the bundle that owns the service):

- `default` — generic PHP front controller. Honors `rewrite_all_to`,
  `asset_fallback`, `front_controller_fallback`, and `error_page_404`.
  Includes static-asset caching, deny rules for hidden/sensitive files,
  gzip, and standard FastCGI tuning.
- `decodelabs` — minimal monolithic front controller used by the
  `decodelabs` bundle. Rewrites every request to `/vendor/genesis.php`
  and hands off to php-fpm. No `try_files`, no asset caching, no security
  locations — DecodeLabs apps handle routing, asset serving, and error
  pages in PHP. The `rewrite_all_to`, `asset_fallback`, and
  `error_page_404` params are not consumed under this variant.
- `laravel`, `spa`, `wordpress` — additional shipped variants tuned for
  those framework patterns.

An explicit repo-local `config = "..."` path on the service overrides the
variant lookup entirely. Use that when a stack needs a custom multi-vhost or
proxy layout that does not fit one of the shipped variants.

### `php-fpm`

PHP-FPM workspace / application server.

- Image: custom Dockerfile (PHP-FPM base).
- Parameters:
  - `version` (default `"8.3"`)
  - `extensions` (default `""`)
  - `document_root` (default `"public"`)
  - `working_dir` (default `"/var/www/html"`)
  - `isolated_dirs` (default `[]`)
  - `node_version` (default `""`)
  - `node_global_packages` (default `""`)
  - `mount_host_composer_home` (default `false`)
  - `mount_shared_composer_auth` (default `true`)
  - `mount_shared_composer_cache` (default `true`)
  - `composer_global_packages` (default `""`)
- Exposed ports: none (FPM socket only).
- Volumes: repo root (bind mount).
- Hot-dir overlays:
  - `isolated_dirs = ["vendor", "node_modules"]` moves those repo-root dirs
    onto named volumes.
  - entries are relative to `working_dir`, so `"packages/foo/vendor"` targets
    `/var/www/html/packages/foo/vendor`.
- Healthcheck: none built in.
- Shell target: yes (`/bin/bash`).
- Depends on: optional `mariadb`, `postgres`, `redis`, `memcached` in the
  same environment.
- Gateway: not itself HTTP-facing; pair with `nginx` for route exposure.

Composer state knobs:

- `mount_host_composer_home = true` revives the old full host Composer-home
  bind. Leave this off unless a repo explicitly wants host-owned global tools.
- `mount_shared_composer_auth = true` mounts Effigy-managed shared auth/config
  state so multiple containers can reuse the same Composer tokens without
  sharing the host `vendor/bin`.
- `mount_shared_composer_cache = true` mounts an Effigy-managed shared Composer
  download cache for faster repeated installs.

The normal path is:

- container-owned Composer home and global packages inside the image/container
- shared auth/config under `~/.effigy/shared/composer/`
- shared cache under `~/.effigy/shared/composer-cache/`
- `COMPOSER_CACHE_DIR` is pinned to `/home/dev/.cache/composer` so Composer
  actually uses the shared cache mount rather than defaulting back under
  `COMPOSER_HOME`

PHP runtime defaults:

- Effigy writes a dev `.ini` fragment into the image.
- It also increases PHP path caching for bind-mounted development trees.
- `opcache` is explicitly enabled for FPM requests.
- `short_open_tag` is explicitly disabled.
- Dev defaults currently set:
  - `realpath_cache_size = 4096K`
  - `realpath_cache_ttl = 600`
  - `short_open_tag = Off`
  - `opcache.enable = 1`
  - `opcache.enable_cli = 0`
  - `opcache.memory_consumption = 256`
  - `opcache.interned_strings_buffer = 16`
  - `opcache.max_accelerated_files = 20000`
  - `opcache.validate_timestamps = 1`
  - `opcache.revalidate_freq = 1`

## Gateway Eligibility Summary

Services that expose a TCP port are eligible for automatic loopback IP
allocation and TCP DNS alias registration through the container gateway when
the environment declares `[containers.<name>.dns]`:

- `postgres` (`5432`), `dbgate` (`3000`), `pgweb` (`8081`), `mariadb` (`3306`),
  `redis` (`6379`), `memcached` (`11211`), `mailpit` (`1025`, `8025`),
  `minio` (`9000`, `9001`), `elasticsearch` (`9200`), `phpmyadmin` (`80`),
  `nginx` (`80`), `workspace-rust-bun` (via `host_ports`).

See [`063-container-system-guide.md`](./063-container-system-guide.md) for
the loopback IP pool, alias hostname behavior, and resolver details.

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
- [`065-underlay-starter.md`](./065-underlay-starter.md)

## Expected Outcome

After this guide, you should be able to:

- pick the right catalog service for the next repo by name, default image,
  and exposed ports
- know which parameters can be set inline under `services.<svc>` without
  reading the fragment source
- predict which services will end up on a loopback IP via the gateway and
  which will not

## Next Step

When adding a service to a repo's substrate, start with the default catalog
parameters in this reference, then narrow only the parameters that matter.
If the repo needs a service shape not listed here, either extract the
closest fragment with `effigy service extract <name>` or declare the service
directly in a user-owned `compose_file` as documented in
[`063-container-system-guide.md`](./063-container-system-guide.md).
