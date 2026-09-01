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

Inspect the active repo bundle source directly:

```sh
effigy bundle inspect
effigy bundle sync
```

Catalog-driven services land in generated compose under
`.effigy/runtime/compose/`; see
[`063-container-system-guide.md`](./063-container-system-guide.md) for the
runtime layout.

## Catalog Layers

Fragments resolve in one fixed order, highest priority first:

1. project override — `<repo>/infra/dev/catalog/<name>/`
2. user override — `~/.effigy/catalog/<name>/`
3. active installed catalog pack — see [Catalog Packs](#catalog-packs)
4. compiled baseline — the fragments listed below, embedded in the binary

The compiled baseline ships with every Effigy install and is permanent. A
machine with no pack store, no `oras`, and no network resolves exactly the
fragments documented here. `effigy service list` names the layer each fragment
came from.

## Catalog Packs

A catalog pack is an independently versioned set of the same fragment
directories, installed into Effigy user state and selected below your
overrides. Packs are optional: nothing needs one, and installing one never
changes override precedence.

```sh
effigy service pack status
effigy service pack install oci://<REPO>@sha256:<DIGEST>
effigy service pack install --path ./catalog-pack
effigy service pack rollback
effigy service pack reset
```

Every shape takes standard leading `--repo` and `--json`.

### Pack shape

A pack root holds `pack.toml` plus fragment directories in the usual layout:

```toml
schema_version = 1

[pack]
id = "effigy-default-catalog"
version = "1.4.0"
description = "Default Effigy service catalog"

[compatibility]
effigy = ">=0.12, <0.13"
```

Fragment files (`service.toml`, `compose.fragment.yml`, `Dockerfile`,
`configs/`, `variants/`) follow
[`071-catalog-service-authoring.md`](./071-catalog-service-authoring.md)
unchanged. A pack cannot widen the fragment schema.

### Acquisition rules

- Installation is always explicit. Ordinary catalog use never fetches, checks
  for updates, or touches the network.
- An `oci://` source must be digest-addressed (`@sha256:...`). A tag-only
  reference is rejected before any transport call.
- `--path <DIR>` installs from a local directory, for development and recovery.
- Acquire, validate, store, and activate are one transaction. Activation
  happens last and only after the manifest, compatibility, and fragments
  validate. A failed candidate leaves the previous selection and previously
  installed content untouched.
- Pack content may contain only regular files and directories with valid UTF-8
  names. A symlink — file or directory, including the pack root and `pack.toml`
  themselves — is rejected before anything is read, hashed, or copied, so a pack
  cannot reach outside its own root. Non-UTF-8 entry names are rejected too:
  lossy names would let two different trees share one content identity.
- The store records pack identity, pack version, manifest schema version, the
  compatibility requirement, the source, the resolved OCI digest, and a
  deterministic content identity over the whole tree.
- Landing and activation are serialized across processes by an advisory lock on
  the store, so concurrent installs cannot lose each other's lineage.
  Acquisition itself stays outside the lock.

### Retention

Every successfully installed pack is retained. `install`, `rollback`, and
`reset` never delete installed content — the prototype has no deletion
authority, and garbage collection or a bounded retention policy is a later
explicit decision. `effigy service pack status` lists everything the store
holds.

Reinstalling content the store already has re-verifies the stored bytes against
their recorded identity. Matching content is reused; content that fails
verification is replaced with the freshly validated candidate rather than
reactivated, and the displaced tree is set aside, not deleted.

### Recovery

`rollback` selects the previous validated install and is a swap, so it returns.
It re-proves that install against the running Effigy first, using exactly the
check selection runs; if the previous content has since been deleted, tampered
with, replaced by a symlink, or become incompatible, `rollback` refuses and
leaves the current selection untouched. `effigy doctor` recommends `rollback`
only when that same proof passes, and recommends `reset` otherwise.

`reset` selects the compiled baseline; it retains installed content and never
touches project or user overrides, so `rollback` still works afterwards. It is
also the recovery path for damaged store metadata: an unreadable or unsupported
`state.json` is copied aside under a `state.json.unreadable-*` name — never
moved or deleted — and the live document is then replaced atomically with a
valid baseline-selected one, so the state file is never briefly absent. If any
step fails, the original file and bytes are left exactly as they were. Selection
pointers naming no retained record are dropped. Install directories are always
kept. Records that lived only in an unreadable document cannot be rebuilt, so
reset reports retained records and retained content separately; the content can
be reinstalled with `--path`.

Selection re-proves the active pack every time, not just on install: it
revalidates the manifest and fragments, cross-checks the stored manifest
against the install record, and re-hashes the tree against the recorded content
identity. Deleted files, edited compose or config bytes, a swapped identity, or
a store pointer with no record behind it all count as unhealthy.

When the active pack is unhealthy, Effigy uses the compiled baseline and says
so on stderr — a `[warn]` line normally, and a single
`effigy.catalog-pack.fallback.v1` object under `--json`. That notice reaches
*every* catalog-backed command, including container, system, workspace, and
task paths that have no selection payload of their own; stdout contracts are
unchanged. `effigy doctor` additionally raises `catalog.pack-health` with one
direct repair command.

### Not in this surface

There is no `effigy service pack update`. The official channel is fixed and
baseline-owned — installed pack content cannot redirect it — but no official
artifact is published yet, so no public update command exists.

Effigy owns `support/catalog-pack-update.toml`, the machine-readable
compatibility set for that future public channel. Only an Effigy
support-policy or release PR may change it. Catalog-pack publication consumes
the file from a resolved Effigy default-branch commit and blob digest; the
pack repository, pack content, and installed state cannot redefine the
required set. Local Effigy validation is network-free and does not affect pack
selection, acquisition, or activation.

## Catalog Services

Each section below covers one shipped fragment. Parameters shown with
`default` are the current built-in defaults; override them inline under the
service definition.

### `workspace-rust-bun`

Long-running Rust + Bun workspace container. Used by `[bundle].base =
"workspace-app"` as the default workspace service and by managed tasks that open
`role = "shell" service = "workspace"`.

- Image: custom Dockerfile (Rust + Bun base).
- Parameters:
  - `rust_version` (default `"1.88"`) — Rust base image tag.
  - `bun_version` (default `"1.3.14"`) — pinned Bun release; the
    Dockerfile downloads the matching `bun-v<version>` GitHub release
    archive and verifies its SHA256 against the release's
    `SHASUMS256.txt`. The build fails when the version is empty.
  - `workspace_mount` (default `"/workspace-root"`) — in-container mount
    point for the workspace root.
  - `working_subdir` (default `""`) — subdirectory of `workspace_mount` used
    as the container's `working_dir`.
  - `host_ports` (default `[]`) — explicit host port bindings (for example
    filled in by a workspace-app bundle from `api_port`/`admin_port`/
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
`workspace-app` bundle.

- Image: `dbgate/dbgate:7.2.3` (override via `version`).
- Parameters: `version` (`"7.2.3"`), `database_host` (`"postgres"`),
  `database_port` (`5432`), `database` (`"postgres"`),
  `database_user` (`"postgres"`), `database_password` (`""`),
  `engine` (`"postgres@dbgate-plugin-postgres"`),
  `connection_label` (`"Postgres"`), `login` (`""`), `password` (`""`).
- Auth: no web-UI login by default. That is only acceptable because
  generated compose binds published ports to `127.0.0.1`; set `login` /
  `password` (DbGate's `LOGIN` / `PASSWORD` env contract) when publishing
  beyond loopback, and source them from the effigy secrets vault.
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

- Image: `sosedoff/pgweb:0.17.0` (override via `version`).
- Parameters: `version` (`"0.17.0"`), `database_host` (`"postgres"`),
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

- Image: `axllent/mailpit:v1.30.6` (override via `version`).
- Parameters: `version` (`"v1.30.6"`), `smtp_port` (`1025`), `ui_port`
  (`8025`).
- Exposed ports: SMTP on `1025`, web UI on `8025`.
- Volumes: none.
- Healthcheck: HTTP `GET /` on the UI port.
- Shell target: no.
- Gateway: eligible for HTTP route (for example `mailpit.<host>`) through
  `[containers.<name>.dns]`; also gets a TCP loopback alias for the SMTP
  port when the bundle wires one.

For shipped PHP workspaces, Effigy also configures a sendmail-compatible
`msmtp` shim so PHP `mail()` calls are forwarded to the local Mailpit SMTP
listener by default.

### `minio`

Local S3-compatible object storage and console.

- Image: `minio/minio:RELEASE.2025-09-07T16-13-09Z` (override via
  `version`; upstream stopped moving `latest` after 2025-09).
- Parameters: `version` (`"RELEASE.2025-09-07T16-13-09Z"`),
  `root_user` (`"minioadmin"`), `root_password` (`"minioadmin"`),
  `api_port` (`9000`), `console_port` (`9001`).
- Exposed ports: S3 API on `9000`, console on `9001`.
- Volume: persistent `data` volume at `/data`.
- Healthcheck: `mc ready local`.
- Shell target: yes (`/bin/sh`).
- Gateway: eligible for HTTPS app-facing routes (for example `https://s3.<host>`
  and `https://minio.<host>`) plus a TCP loopback alias for the raw S3 port.

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

- Image: `phpmyadmin:5.2.3` (override via `version`).
- Parameters: `version` (`"5.2.3"`), `database_host` (`"db"`),
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
- `php-app` — minimal monolithic front controller used by the
  `php-app` bundle. Rewrites every request to `/vendor/genesis.php`
  and hands off to php-fpm. No `try_files`, no asset caching, no security
  locations — PHP app apps handle routing, asset serving, and error
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
  - `host_ports` (default `[]`) — explicit host port bindings for temporary
    workspace-side servers such as a Vite/Zest dev process
  - `node_version` (default `""`)
  - `node_global_packages` (default `""`)
  - `mount_host_composer_home` (default `false`)
  - `mount_shared_composer_auth` (default `true`)
  - `mount_shared_composer_cache` (default `true`)
  - `composer_global_packages` (default `""`)
- Exposed ports: none by default; explicit `host_ports` can publish
  workspace-side development servers.
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
- `mount_shared_composer_auth = true` mounts one Effigy-managed shared
  Composer-home volume at `/home/dev/.config/composer` so PHP workspaces can
  reuse the same auth, config, and Composer global state without binding the
  host Composer directory.
- `mount_shared_composer_cache = true` mounts an Effigy-managed shared Composer
  download cache for faster repeated installs.

The normal path is:

- shared Composer home on a named Docker volume
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
- [`065-external-bundle-adoption.md`](./065-external-bundle-adoption.md)

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
