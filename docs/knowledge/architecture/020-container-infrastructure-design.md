# Container Infrastructure Design

Effigy's local container infrastructure combines catalog assembly, workspace
execution, managed sessions, gateway routing, and data lifecycle. This document
keeps the detailed design and ownership boundaries together. For command syntax
use the [container guide](../../guides/063-container-system-guide.md); for exact
runtime guarantees use [contract 005](../contracts/005-container-runtime-contract.md)
and [contract 009](../contracts/009-execution-surface-convergence.md). The
[package map](010-package-map.md) owns current module locations.

The container backend runs Compose against Colima or Docker. A container
manifest may declare catalog services or point at a repo-owned Compose file.
Generated compose receives the richer data, cache, and lifecycle management
because Effigy owns its shape. A repo-owned Compose file remains a supported
escape hatch with narrower guarantees.

## Architecture Overview

The local environment is assembled from these cooperating layers:

```
+-----------------------------------------------------+
|  effigy manifest (effigy.toml)                       |
|  +-----------------------------------------------+  |
|  | [containers.web]                               |  |
|  |   services.app = { catalog = "php-fpm", ... }  |  |
|  |   services.web = { catalog = "nginx", ... }    |  |
|  |   dns.routes = [{ domain = "project.test" }]  |  |
|  | [systems.dev.workspaces.app]                   |  |
|  |   container = "web"                            |  |
|  +-----------------------------------------------+  |
+-----------------------------------------------------+
|  effigy runtime                                      |
|  +-----------+ +----------+ +----------+ +--------+  |
|  | Catalog / | | Context  | | Gateway  | | Data   |  |
|  | Compose   | | Routing  | | DNS +    | | Volume |  |
|  | Assembly  | | + Exec   | | Proxy +  | | Mgmt   |  |
|  |           | |          | | HTTPS    | |        |  |
|  +-----------+ +----------+ +----------+ +--------+  |
+-----------------------------------------------------+
|  container backend (Colima or Docker + Compose)      |
+-----------------------------------------------------+
```

## 1. Service Catalog and Compose Assembly

### Problem

A repo-owned Compose environment can require hand-written Compose files
and Dockerfiles for every project. For a PHP project needing nginx, MariaDB, Redis, and Memcached,
that's significant boilerplate that gets copied between projects and maintained
in parallel.

### Solution

Effigy ships a **service catalog** — a collection of composable service
fragments that are assembled into a Compose document from manifest
declarations. The catalog is just files (compose snippets, Dockerfiles, config
files). Effigy's Rust code knows nothing about PHP, nginx, or MySQL. It knows
how to read fragment metadata, substitute parameters, and assemble compose
files.

### Catalog structure

```
catalog/
  php-fpm/
    compose.fragment.yml        # templated compose service definition
    Dockerfile                  # accepts build args for version, extensions
    service.toml                # parameter schema, defaults, capabilities
  nginx/
    compose.fragment.yml
    service.toml
    configs/
      default.conf              # generic PHP front-controller passthrough
      laravel.conf              # Laravel try_files + front controller
      wordpress.conf            # WordPress rewrite rules
      spa.conf                  # SPA with API proxy
  mariadb/
    compose.fragment.yml
    service.toml
    configs/
      my.cnf                    # sensible defaults
  postgres/
    compose.fragment.yml
    service.toml
  redis/
    compose.fragment.yml
    service.toml
  memcached/
    compose.fragment.yml
    service.toml
```

Each fragment's `service.toml` declares its parameter interface:

```toml
# php-fpm/service.toml
name = "php-fpm"
description = "PHP-FPM application server"

[params]
version = { type = "string", default = "8.3" }
extensions = { type = "list", default = [] }
document_root = { type = "string", default = "public" }
working_dir = { type = "string", default = "/var/www/html" }

[capabilities]
exec_target = true
shell = "/bin/bash"
```

### Manifest declaration

```toml
[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
extensions = ["pdo_mysql", "gd", "redis", "memcached", "intl", "exif"]
document_root = "public"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
# OR: config = "infra/nginx.conf" for custom configs

[containers.web.services.db]
catalog = "mariadb"
version = "10.11"

[containers.web.services.cache]
catalog = "redis"

[containers.web.services.sessions]
catalog = "memcached"
```

### Assembly flow

1. Read service declarations from manifest.
2. Load matching catalog fragments (project-local > user-global > active pack > bundled baseline).
3. Substitute parameters into fragment templates.
4. Assemble into a complete compose file.
5. Write to `.effigy/runtime/compose/.effigy-compose.generated.yml` (gitignored).
6. If `.effigy/runtime/compose/compose.override.yml` exists, pass both files
   to Docker Compose (`-f generated.yml -f override.yml`).
7. Regenerate on manifest change (checksum comparison).

### Catalog distribution

The baseline catalog fragments are **embedded in the Effigy binary**. The
assembler reads the selected layer and writes the generated Compose file plus
rendered assets under `.effigy/runtime/compose/.effigy-catalog/`. A consumer
does not need a separate catalog checkout or network access for the baseline.

User customization layers on top:

1. **Project-local:** `infra/dev/catalog/` in the repo
2. **User-global:** `~/.effigy/catalog/`
3. **Active catalog pack:** an explicitly installed, versioned pack
4. **Bundled baseline:** embedded in the binary (lowest priority)

`effigy service list` shows available services. `effigy service extract
<service>` extracts a bundled fragment to the override directory for
customization.

### Eject

`effigy container <name> eject` copies the generated compose file into a repo-owned compose file, switches the manifest to `compose_file =`, and the user
owns it directly. Generated-compose data commands no longer apply after eject.

### Design reference: DDEV

This model absorbs key patterns from DDEV:

- **Compose generation with layered overrides** (not static compose files)
- **Complexity lives in Docker images, not the orchestrator** (the PHP
  Dockerfile knows about extensions; effigy doesn't)
- **Framework-specific web configs are just files** selected by a parameter
  (the web-server config stays a selected file)
- **Composable add-on model** (additional services are compose fragments)

What this explicitly does NOT absorb from DDEV:

- DDEV as a dependency or driver
- DDEV's framework detection / auto-configuration
- DDEV's hosting provider integration
- DDEV's Traefik router (replaced by a Rust-native gateway)

### Nginx config flexibility

The nginx fragment ships named config variants (default, laravel, wordpress,
spa) selected via `variant =` in the manifest, plus explicit params for
rewrite/fallback behavior. For custom frameworks, the user either:

- provides their own config via `config = "infra/nginx.conf"`
- extracts the default variant and modifies it
- adds a new variant to their project-local catalog

The `default.conf` variant is a simple PHP front-controller passthrough that
works for most custom frameworks using `mod_rewrite`-style routing:

```nginx
location / {
    try_files $uri $uri/ /index.php?$query_string;
}
```

Genesis-style apps that do not use a `public/` front controller can stay on
the generic nginx config and set explicit params instead, for example:

```toml
[containers.web.services.web]
catalog = "nginx"
document_root = "."
rewrite_all_to = "/vendor/genesis.php"
asset_fallback = "/vendor/genesis.php"
error_page_404 = "/vendor/genesis.php"
```

## 2. Workspace Execution

A system can select a default workspace backed by a container:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

```

The built-in `test` command is configured under `[test]`, not as a task.
Ordinary repository tasks may set
`workspace = "app"` or `run_in = "container"`; `run_in = "host"` keeps a task
on the host. The selected system, workspace, execution binding, and effective
container policy determine the target. Built-ins that inspect or manage the
host remain host owned. Routing is explicit, and ambiguous or invalid targets
fail before side effects.

The host runner detects when it is already inside the workspace container. A
container handoff executes Effigy there when present; otherwise host-owned
transport runs the command with the mapped working directory and declared
workspace identity. `effigy exec` is the explicit ad-hoc path:

```sh
effigy exec composer install
effigy exec php artisan migrate
effigy exec --service db mysql
```

For the primary service, host-routed tasks and `effigy exec` use
`workspace_user` and `workspace_home`. An explicitly selected other service
retains its own user. Interactive terminals get a TTY; pipes and agents do
not. Host-to-container cwd mapping follows the declared mount, rather than
assuming the caller started at repository root.

Aliases provide named access to tools in another service:

```toml
[containers.web.aliases]
mysql = { service = "db", command = "mysql" }
redis-cli = "cache"
```

Host-visible aliases and container-local aliases have distinct installation
and cleanup owners. [Contract 005](../contracts/005-container-runtime-contract.md)
sets their precedence and safety rules. Most project commands remain normal
manifest tasks; aliases are for small interactive tools.

## 3. Dev Front Door and Managed Lifecycle

`effigy dev` is a repository task, normally a managed concurrent task with a
lifecycle process and workspace shell. The task selector resolves through the
ordinary task graph; the runner does not reserve `dev` as a built-in. Managed
mode gives the operator tabs for process output and a terminal, readiness
feedback, and an interrupt-aware closeout path.

There are two activation owners:

1. **Public shell sessions** (`effigy dev`, `effigy workspace`, and
   `stay_in_shell` tasks) prepare the runtime, reconcile gateway routes before
   opening the shell, then ask whether to bring the container down at exit.
   The prompt defaults to yes. Attached `effigy container up` still follows
   `on_task_exit`.
2. **Non-shell tasks** (`run_in = "container"`, deferred container requests,
   and bootstrap tasks) auto-start a needed runtime, bring up sibling services,
   wait for exec readiness, reconcile declared routes, and refresh a host
   lease. The default lease is five minutes; reuse refreshes it. An explicit
   `container up` or owned shell session keeps the runtime up beyond that
   lease.

The activation pipeline is typed in `effigy-runtime-plan`. The runner executes
its stages through adapters: policy validation, runtime start, mount
preparation, sibling-service readiness, exec readiness and recovery, gateway
and alias reconciliation, and lease refresh. Cleanup authority depends on
whether a public session or a task lease owns the environment. This avoids a
container task unexpectedly tearing down a workspace still in use.

`effigy container down` tears down the Compose environment; it does not stop
the Colima profile. `startup = "attached"` makes `container up` session owned,
with graceful shutdown on interrupt. Detached startup returns after readiness.

## 4. Host Gateway, DNS, HTTP, and TCP Aliases

The Rust gateway is a host process. It answers local DNS names, proxies HTTP
and HTTPS by host, and serves loopback TCP aliases for catalog services. It is
not a container and does not depend on a particular application language.

Main owners in `effigy-gateway`:

| Module | Responsibility |
| --- | --- |
| `dns` | local DNS answers and query accounting |
| `proxy` | HTTP/HTTPS forwarding, WebSocket, timeouts, graceful drain |
| `tls` | rustls and mkcert-backed certificate selection |
| `routes` / `registration` | atomic route table and lifecycle registration |
| `trust` | elevated read-path ownership, mode, and marker checks |
| `resolver_setup` | macOS resolver file management |
| `ports` | persistent project port allocation |
| `tcp_alias` / `loopback` | local aliases and safe loopback binding |
| `server` / `stats` | daemon lifecycle and operator statistics |

A manifest route can target a container service or a declared host listener:

```toml
[containers.web.dns]
[[containers.web.dns.routes]]
domain = "app.test"
service = "web"
port = 80
tls = true
```

`domains = ["app.test"]` with `domain_defaults` is shorthand for repeated
routes; an explicit route for the same domain wins. `target_host` takes a
`host:port` target and cannot be combined with `service` in one route. The
hostname for concurrent worktrees remains an [open lead](../../triage/20260917-230600-per-worktree-container-identity-default.md),
not a convention established by this design.

The route table at `~/.effigy/gateway/routes.json` maps domain names to
upstream, DNS, TCP alias, TLS, source, and project facts. Writers replace it
atomically. It carries an Effigy-managed marker and owner-only permissions.
The elevated daemon checks ownership, permissions, and marker on initial load
and every reload. It refuses an untrusted table and keeps the last known good
routes. This matters because the daemon can bind `:80`/`:443` and proxy to
arbitrary upstreams; see [gateway trust contract](../contracts/033-gateway-route-table-trust-contract.md).

A saved route table is a map keyed by domain, not the old list form. A route
contains the domain, optional HTTP `target`, optional `dns_ip`, optional TCP
alias `tcp_port`/`tcp_target`, `source`, absolute project path, `tls`, and the
registration time. DNS-only aliases omit the HTTP target. The on-disk envelope
also carries `_managed_by`; callers must not hand-edit that marker.

The gateway watches the file and swaps a verified table into memory. Atomic
writes prevent a reader from seeing a partial JSON document. Trust validation
runs again on reload, so a file that becomes writable by another user cannot
be adopted simply because the daemon once trusted it. An untrusted update
retains the previous good in-memory table and appears in gateway status and
doctor output.

On macOS, gateway setup manages `/etc/resolver/` files for local domains.
HTTPS uses mkcert-backed certificates after `effigy gateway setup-tls`.
Plain HTTP redirects to HTTPS for TLS routes. `.test` names work
without TLS for local development. `.dev` names require HTTPS in common
browsers because of HSTS, so a `.dev` route needs certificate setup before it
is useful. The gateway serves certificates; mkcert manages the local trust
chain. The gateway can also serve
non-container host listeners when the manifest explicitly declares them;
route lifecycle remains tied to the owning environment. DNS-only TCP aliases
for services such as Postgres, MariaDB, Redis, and Memcached use deterministic
loopback targets, avoiding manual `/etc/hosts` edits.

## 5. Persistent Data, Cache, and Volume Lifecycle

Generated Compose gives Effigy enough ownership to classify data safely.
Catalog fragments declare their named volumes. Project-scoped names prevent
accidental cross-project reuse. `container down` preserves data; `reset`
preserves data unless `--wipe-data` is explicit. `reset --keep-data` remains a
supported explicit preservation path. Import, production pull, and wipe
operations require confirmation; non-interactive callers use `--yes`.

The data surface covers `list`, `export`, `import`, `pull-production`, logical
SQL `dump`, and `seed`. `dump` writes local files by default. OCI destinations
are staged first and published only with `--push`, which reports the resulting
digest. `seed` stages local or `oci://` artifacts under
`.effigy/local/db-seeds/`, then dispatches the repository's
`bootstrap:db-seed` task. It currently targets the default container. The
[container guide](../../guides/063-container-system-guide.md#data-lifecycle)
shows command syntax and confirmation rules. The main shapes are:

```sh
effigy container data list
effigy container data export mysql-data ./mysql-data.tar
effigy container data import mysql-data ./mysql-data.tar --yes
effigy container data dump app=./app.sql
effigy container data dump app=oci://ghcr.io/acme/uat:2026-09-26 --push
effigy container data seed --db-seed app=oci://ghcr.io/acme/uat:2026-09-26
```

Import and production pull can overwrite local data. A JSON or non-interactive
caller gets no prompt and must pass `--yes` for an intentional mutation.
Dumping to an OCI address without `--push` only plans and stages the payload;
a digest-pinned reference is not a writable destination.

`effigy-data` owns pure target selection and database command plans. The runner
owns prompting, artifact adapters, container execution, and report rendering.
The shared database resolver classifies Postgres, MariaDB, and MySQL catalog
entries, reads declared databases and credential references, and rejects
missing or ambiguous targets. Secret values stay out of reports. See the
[database resolution contract](../contracts/026-shared-database-target-resolution-contract.md).

Cache and persistent volume cleanup are separate. Generated Compose can put
Rust `target`, `node_modules`, and pnpm store paths on disposable named
volumes. `container cache list/prune` classifies those build caches;
`container volume list/prune` handles named volume ownership, dormant repo
volumes, and global orphans. Running projects are skipped by global cache
prune. Persistent application data is never swept as a dormant or orphan
cache. Explicit destructive operations require confirmation.

```sh
effigy container cache list --global
effigy container cache prune --kind rust-target --yes
effigy container volume list --dormant
effigy container volume list --global --orphans
effigy container volume prune --global --orphans --yes
```

Cache classification uses mount targets as well as names so older opaque
`efv-*` volumes can still be recognized. A named volume under an application
data path is never reclassified as disposable only because it is idle.

External host mounts use structured tables with `external = true`; legacy
string mounts remain repo-relative. Generated published ports bind to
`127.0.0.1` by default; a container may opt into `publish_address =
"0.0.0.0"`. These defaults protect local databases and admin endpoints from
unintentional LAN exposure.

## 6. Multi-Project Coordination

The port registry at `~/.effigy/ports.json` allocates project ranges and
stable service offsets. Explicit host-port declarations take precedence.
Per-project allocations and loopback publishing allow several environments to
run together without hand-assigned ports. The registry starts its
default allocation pool at 8100 with 100 ports per project. Standard offsets
include HTTP `+0`, MariaDB `+6`, Postgres `+32`, Redis `+79`, and Memcached
`+11`. Assignments are keyed by service and container port, so an unchanged
binding can keep its host port within the project's range. Exhaustion is an
explicit error; a new service does not silently take another project's port.

`container status --global`,
`container stats --global`, `container cache list --global`, and
`container volume list --global` expose machine-wide state; gateway status
shows registered routes.

A generated service may declare `shared = true` for bounded shared backing
services. The policy rejects unsupported variant/config combinations and
requires at least one local service. Sharing trades isolation for resource
use; it is explicit, not an automatic optimization.

The gateway domain remains globally keyed by hostname. Two worktrees with the
same configured domain can collide; the [open lead](../../triage/20260917-230600-per-worktree-container-identity-default.md)
records the unresolved identity/default question. Port allocation alone does
not solve that collision.

## Crate and Module Ownership

| Owner | Role |
| --- | --- |
| `effigy-catalog` | fragment schema, layered lookup, template rendering, compose assembly, output/eject, volume classification, catalog packs |
| `effigy-manifest` | systems, workspaces, containers, DNS routes, services, lifecycle, host, data, and alias grammar |
| `effigy-containers` | effective policy, generated Compose, manager facade, backend operations, workspace transport |
| `effigy-exec` | host/container route decision, cwd translation, aliases, capability detection, readiness model |
| `effigy-runtime-plan` | typed activation stages and policy |
| `effigy-runtime` | runtime read/write/data/shell adapters |
| `effigy-data` | database target and seed/dump domain plans |
| `effigy-gateway` | DNS, proxy, TLS, route trust/registration, port registry, TCP aliases |
| `effigy-managed` / `effigy-tui` | managed task process and tabbed operator session |
| runner | CLI dispatch, prompts, side effects, output, cleanup, and gateway reconciliation |

The current runner module locations are in the [package map](010-package-map.md).
The [runtime operation contract](../contracts/015-runtime-operation-pipeline-contract.md)
defines request, plan, adapter, and report responsibilities. Pure decisions
belong below the runner; host side effects and human prompts remain at its
edge.

## Example Manifest Boundary

```toml
[containers]
default = "web"

[containers.web]
driver = "colima"
profile = "effigy"
project_name = "client-dev"
primary_service = "app"
startup = "attached"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[containers.web.services.db]
catalog = "mariadb"

[containers.web.dns]
[[containers.web.dns.routes]]
domain = "client.test"
service = "web"
port = 80

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"

[containers.web.host]
mounts = ["./:/var/www/html"]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks.seed]
workspace = "app"
run = "rhai:scripts/seed.rhai"
```

The manifest declares local infrastructure and task binding, not a production
provider topology. `compose_file` can replace the generated service block
when the repository needs direct Compose ownership. Release and provider
export remain separate surfaces.

## Design Boundaries

- The catalog is data and templates. Rust assembly knows how to validate and
  merge fragments; it does not contain PHP, nginx, or database-specific
  branching for application behavior.
- The effective manifest and selected catalog determine generated Compose.
  Live container inspection cannot silently rewrite that source of truth.
- The gateway is a localhost developer tool, not a multi-tenant proxy.
  Owner/marker checks protect against foreign writes; they do not defend
  against the same UID or root.
- Persistent data, disposable cache, and orphan volume cleanup have distinct
  ownership and confirmation rules.
- Public shell closeout, non-shell task leases, and detached container
  operations have distinct lifecycle owners.
- Service and domain names are explicit. No framework detection or
  provider deployment assumption follows from a local catalog declaration.
