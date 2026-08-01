# Container Infrastructure Design

Status: background design reference
Updated: 2026-08-01

## Purpose

This document captures the full architectural design for effigy's container
infrastructure layer — the system that turns the v1 container surface into a
complete local development environment platform.

Current ownership truth for the live runtime/container core no longer lives
 here. Use these as the current authority surfaces instead:

- [010-package-map.md](./010-package-map.md) for current crate and module
  ownership
- `docs/contracts/005-container-runtime-contract.md` for local runtime guarantees
- `docs/contracts/009-execution-surface-convergence.md` for cross-surface
  execution responsibilities

The v1 container surface (`g02.006`) shipped a narrow, trustworthy foundation:
named container environments, Colima + Docker Compose lifecycle, attached
sessions, and task integration through the released `system` / `workspace`
execution contract. This design extends that
foundation into a system where any project can declare its service stack in the
manifest, get a working environment from a single command, and interact with it
transparently from the host.

## Design Thread Context

This design was developed in a dedicated planning thread separate from the
active modularization thread (`g02.010`). The two threads are intentionally
isolated:

- the modularization thread owns `g02.006`–`g02.010` and the main runner
- this thread owns `g02.011`–`g02.016` and writes only isolated library crates
- final integration into the main runner happens after modularization completes
- the human coordinates between the two threads where needed

If this thread is lost, another thread can pick up from this document plus the
roadmap and spec files listed below.

## Governing Refs

- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/011-service-catalog-and-compose-assembly.md`
- `docs/roadmaps/g02/012-container-context-and-transparent-execution.md`
- `docs/roadmaps/g02/013-dev-front-door-and-managed-lifecycle.md`
- `docs/roadmaps/g02/014-rust-native-gateway.md`
- `docs/roadmaps/g02/015-persistent-data-and-volume-lifecycle.md`
- `docs/roadmaps/g02/016-multi-project-coordination.md`
- `docs/specs/011-service-catalog-and-compose-assembly-strict-lane.md`
- `docs/specs/006-colima-container-environment-strict-lane.md` (predecessor)
- `docs/guides/063-container-system-guide.md` (v1 operator guide)

## Architecture Overview

Four new concepts layer on top of the v1 container surface:

```
+-----------------------------------------------------+
|  effigy manifest (effigy.toml)                       |
|  +-----------------------------------------------+  |
|  | [containers.web]                               |  |
|  |   services.app = { catalog = "php-fpm", ... }  |  |
|  |   services.web = { catalog = "nginx", ... }    |  |
|  |   dns.domain = "project.test"                  |  |
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
|  v1 container surface (Colima + Docker Compose)      |
+-----------------------------------------------------+
```

## 1. Service Catalog and Compose Assembly

### Problem

The v1 surface requires a hand-written `docker-compose.yml` and Dockerfiles for
every project. For a PHP project needing nginx, MariaDB, Redis, and Memcached,
that's significant boilerplate that gets copied between projects and maintained
in parallel.

### Solution

Effigy ships a **service catalog** — a collection of composable service
fragments that are assembled into a docker-compose.yml from manifest
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
2. Load matching catalog fragments (project-local > user-global > bundled).
3. Substitute parameters into fragment templates.
4. Assemble into a complete compose file.
5. Write to `.effigy/runtime/compose/.effigy-compose.generated.yml` (gitignored).
6. If `infra/dev/compose.override.yml` exists, merge via Docker Compose
   multi-file (`-f generated.yml -f override.yml`).
7. Regenerate on manifest change (checksum comparison).

### Catalog distribution

Catalog fragments are **embedded in the effigy binary** at compile time. At
runtime, effigy reads from its internal catalog directly — no extraction step,
no external file management, no stale files.

User customization layers on top:

1. **Project-local:** `infra/dev/catalog/` in the repo
2. **User-global:** `~/.effigy/catalog/`
3. **Bundled:** embedded in the binary (lowest priority)

`effigy service list` shows available services. `effigy service extract
<service>` extracts a bundled fragment to the override directory for
customization.

### Eject

`effigy container eject` copies the generated compose file into a permanent
`docker-compose.yml`, switches the manifest to `compose_file =`, and the user
owns it directly. No lock-in.

### Design reference: DDEV

This model absorbs key patterns from DDEV:

- **Compose generation with layered overrides** (not static compose files)
- **Complexity lives in Docker images, not the orchestrator** (the PHP
  Dockerfile knows about extensions; effigy doesn't)
- **Framework-specific web configs are just files** selected by a parameter
  (DDEV ships 26 nginx/apache configs as static files)
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

## 2. Default Workspace Execution

### Problem

With v1, running project commands inside the container requires explicit
`effigy container web shell --command "..."` invocations. For projects where
most work happens inside the container, this is friction.

### Solution

A default system workspace can point at a backing container. When set, Effigy
implicitly routes task execution through that workspace container.

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
```

### Routing logic

```
Does the default system workspace resolve to a backing container?
  Yes -> Is this a host-native command?
           (doctor, container, release, gateway, tasks, catalog)
           Yes -> run on host
           No  -> Is the container running?
                    Yes -> exec inside the container
                    No  -> prompt or auto-start based on policy
  No  -> run on host (current v1 behavior)
```

Individual tasks can override:

- `run_in = "host"` — forces host execution
- `workspace = "other"` plus system resolution — targets a different workspace
  and therefore a different backing container

### Effigy-in-container handoff

If the container image has effigy installed, the host effigy hands off entirely:

```
docker compose exec app effigy test
```

The container effigy runs natively with full access to the runtime. No CWD
mapping needed — paths resolve naturally.

If effigy is not in the container, the host does:

```
docker compose exec -w <mapped-cwd> app <raw command>
```

CWD mapping translates the host path to the container working directory based
on the mount configuration.

### `effigy exec`

The explicit catch-all for ad-hoc commands:

```bash
effigy exec composer install
effigy exec php artisan migrate
effigy exec --service db mysql
```

### Exec aliases

For bare tool access that isn't an effigy task:

```toml
[containers.web.exec]
working_dir = "/var/www/html"

[containers.web.exec.aliases]
mysql = { service = "db", command = "mysql" }
redis-cli = { service = "cache", command = "redis-cli" }
```

These are a small, explicit surface for interactive tools. Most project
commands route through the container context automatically via normal tasks.

## 3. Dev Front Door and Managed Lifecycle

### Problem

Starting a dev environment should be one command, not a sequence of
`container up`, `gateway up`, health check, open browser.

### Solution

`effigy dev` is a task that uses the managed-process concurrent runtime to
provide a unified TUI experience:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks.dev]
workspace = "app"
mode = "attached"
gateway = true
health_wait = true
ready_message = "http://clientname.test"

[[tasks.dev.concurrent]]
name = "services"
label = "Container"
role = "lifecycle"
shutdown_on_exit = true

[[tasks.dev.concurrent]]
name = "terminal"
label = "Shell"
role = "shell"
```

### Lifecycle

1. Container starts (compose assembly from catalog if needed).
2. Gateway starts (if DNS configured and not already running).
3. Health check runs.
4. TUI opens with service overview + embedded terminal.
5. "Ready at http://clientname.test" appears when health passes.
6. User works in the terminal tab or from another host terminal.
7. Close TUI -> graceful shutdown -> DNS deregisters -> machine is clean.

### Relationship to v1 attached sessions

This builds on v1's attached-session model. The container lifecycle is the
managed process. The TUI is the feedback surface. The managed-process runtime
(from `g02.010` modularization) provides the concurrent tab model.

## 4. Rust-Native Gateway

### Problem

Running projects need discoverable local domains. `localhost:8080` works but
doesn't scale to multiple projects and doesn't match production URLs.

### Solution

A Rust-native background process that provides DNS resolution and reverse
proxying for local development domains.

### Components

- **DNS resolver** (`hickory-dns`): responds to `*.test` queries with
  `127.0.0.1`
- **Reverse proxy** (`hyper` / `tower`): routes by `Host` header to the
  correct project port
- **TLS termination** (optional): serves mkcert certificates for HTTPS

### Runtime model

The gateway runs as a host-native background process, not a container.

```bash
effigy gateway up       # starts background process
effigy gateway down     # stops it
effigy gateway status   # shows registered routes
```

### Route table

File-based coordination at `~/.effigy/gateway/routes.json`:

```json
{
  "routes": [
    {
      "domain": "clientname.test",
      "target": "localhost:8080",
      "source": "container",
      "project": "/Users/tom/projects/client-x",
      "registered": "2026-04-16T10:00:00Z"
    }
  ]
}
```

`effigy container up` writes entries. `effigy container down` removes them. The
gateway watches the file for changes.

### macOS resolver integration

On first `gateway up`, effigy writes `/etc/resolver/test`:

```
nameserver 127.0.0.1
port 15353
```

Standard macOS convention. No system daemon modification. Requires one `sudo`
prompt with clear explanation. Removed on `gateway down`.

### TLS and HTTPS

`.test` is IETF-reserved (RFC 6761) and doesn't require HTTPS. `.dev` is
Google-owned and Chrome forces HSTS, so it only works with TLS.

Default: `.test` without TLS.
Opt-in: `.dev` (or `.test` with HTTPS) when mkcert certificates are configured.

```toml
[containers.web.dns]
domain = "clientname.test"
tls = true    # requires mkcert on host
```

`effigy gateway setup-tls` — one-time mkcert CA installation helper.

The gateway uses mkcert-generated certificates for TLS termination. Caddy-style
automatic cert generation is not needed — mkcert handles the CA trust chain,
the gateway just serves the certs.

### Non-container project support

The gateway is port-agnostic. For non-containerized projects, route
registration ties to task lifecycle:

```toml
[tasks.dev]
gateway_route = { domain = "myrust.test", port = 3000 }
run = "cargo run"
```

When the task starts, the route registers. When it stops, the route
deregisters. The gateway doesn't know or care what's behind the port.

### New crate

`effigy-gateway` — DNS resolver + reverse proxy + optional TLS. Compiled into
the effigy binary as a subcommand. Runs as a forked background process.

## 5. Persistent Data and Volume Lifecycle

### Problem

Database data and media uploads need to survive container restarts. They also
need to be exportable for machine migration and importable for team
onboarding.

### Solution

Named Docker volumes for persistent state, with explicit lifecycle management.

### Manifest declaration

```toml
[containers.web.data]
volumes = ["mysql-data", "redis-data"]
media = ["storage/uploads:/var/www/html/storage/uploads"]
```

Stack catalog fragments automatically declare appropriate named volumes (the
mariadb fragment creates a data volume by default).

### Lifecycle rules

- `effigy container down` — preserves volumes (Docker Compose default)
- `effigy container reset` — removes everything including volumes
- `effigy container reset --keep-data` — removes containers/images, preserves
  named data volumes

### Volume management commands

```bash
effigy container data list                   # volume names, sizes
effigy container data export <volume> <path> # tar volume to host
effigy container data import <volume> <path> # restore from tar
```

### Seeding as tasks

Seeding is not a special abstraction. It's a normal task that runs inside the
container:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks.seed]
workspace = "app"
run = "rhai:scripts/seed.rhai"

[tasks."seed:fresh"]
workspace = "app"
run = [
    "php artisan migrate:fresh",
    "rhai:scripts/import-migration-bundle.rhai",
]
```

Complex seeding workflows (migration bundles with SQL, media, structured
ingest protocols) are Rhai scripts with access to effigy's exec surface.

### Production data hooks

```toml
[containers.web.data]
pull_production = "rhai:scripts/pull-prod.rhai"
```

Uses Rhai integration for access to effigy context, environment variables
(including `@sensitive` values from env schema), and multi-step orchestration.
Shell scripts also supported as the simple path.

### Volume scoping

Data volumes are project-scoped (prefixed with compose project name). No
cross-project volume sharing to avoid accidental data leaks.

## 6. Multi-Project Coordination

### Problem

Running multiple client projects simultaneously causes port conflicts and
makes it hard to see what's running.

### Solution

Port allocation registry and cross-project visibility.

### Port registry

`~/.effigy/ports.json` maps project names to port ranges:

```json
{
  "allocations": {
    "client-x": { "base": 8100, "range": 100 },
    "client-y": { "base": 8200, "range": 100 }
  }
}
```

When `host.ports` aren't explicitly declared in the manifest, effigy auto-
assigns from the pool. Explicit `host.ports` always wins.

### Cross-project visibility

```bash
effigy container status --global    # all running containers across repos
effigy gateway status            # all registered routes
effigy container stats           # CPU/memory per project
```

### Shared services (optional)

For environments where resource efficiency matters more than isolation:

```toml
[containers.web.services.db]
catalog = "mariadb"
shared = true    # uses a shared instance instead of per-project
```

Opt-in, clearly documented as trading isolation for resource efficiency.

## Crate Map

New crates, all isolated library crates with no dependency on the main runner:

| Crate | Purpose | Milestone |
|-------|---------|-----------|
| `effigy-catalog` | Fragment loading, parameter validation, template rendering, compose assembly | M1 |
| `effigy-gateway` | DNS resolver, reverse proxy, TLS termination, route table management | M4 |

Extensions to existing crates (after modularization completes):

| Crate | Extension | Milestone |
|-------|-----------|-----------|
| `effigy-containers` | Workspace routing, exec proxy, DDEV-pattern lifecycle | M2 |
| `effigy-manifest` | Service declarations, DNS config, data config, system/workspace binding | M1-M5 |
| `effigy-cli` | `exec`, `gateway`, `catalog` command surface | M1-M4 |

## Milestone Sequence

1. `g02.011` — Service Catalog and Compose Assembly
2. `g02.012` — Container Context and Transparent Execution
3. `g02.013` — Dev Front Door and Managed Lifecycle
4. `g02.014` — Rust-Native Gateway (DNS + Reverse Proxy + HTTPS)
5. `g02.015` — Persistent Data and Volume Lifecycle
6. `g02.016` — Multi-Project Coordination

Each milestone is independently valuable. M1 is the critical path — without
it, every project still needs hand-written compose files.

## Full Manifest Example

A real PHP client project using all features:

```toml
[containers.web]
driver = "colima"
primary_service = "app"
startup = "attached"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
extensions = ["pdo_mysql", "gd", "redis", "memcached", "intl", "exif", "zip"]

[containers.web.services.web]
catalog = "nginx"
config = "infra/nginx.conf"

[containers.web.services.db]
catalog = "mariadb"
version = "10.11"

[containers.web.services.cache]
catalog = "redis"

[containers.web.services.sessions]
catalog = "memcached"

[containers.web.dns]
domain = "clientname.test"
tls = true

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"

[containers.web.health]
check = "http://localhost:80"
timeout_secs = 30

[containers.web.data]
volumes = ["mariadb-data"]
media = ["storage/uploads:/var/www/html/storage/uploads"]
pull_production = "rhai:scripts/pull-prod.rhai"

[containers.web.exec]
working_dir = "/var/www/html"

[containers.web.exec.aliases]
mysql = { service = "db", command = "mysql" }

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks.dev]
workspace = "app"
mode = "attached"
gateway = true
health_wait = true
ready_message = "https://clientname.test"

[[tasks.dev.concurrent]]
name = "services"
label = "Container"
role = "lifecycle"
shutdown_on_exit = true

[[tasks.dev.concurrent]]
name = "terminal"
label = "Shell"
role = "shell"

[tasks.test]
run = "php vendor/bin/phpunit"

[tasks.seed]
workspace = "app"
run = "rhai:scripts/seed.rhai"

[tasks."data:rebuild"]
workspace = "app"
run = "rhai:scripts/rebuild-data.rhai"
```

## Developer Workflow

```bash
cd ~/projects/client-x
effigy dev                          # TUI opens, container starts, gateway
                                    # registers, health check runs
                                    # "Ready at https://clientname.test"

# terminal tab is already open inside the container
# or from any other host terminal:
effigy test                         # routes through container automatically
effigy seed                         # runs Rhai seeding script
effigy data:rebuild                 # runs migration bundle
effigy mysql                        # drops into MariaDB shell
effigy exec composer require foo    # ad-hoc commands

# close TUI -> clean shutdown -> machine is clean
```

## Implementation Status

All library crates are shipped as isolated workspace members with no
dependency on other effigy crates. Integration into the runner happens
after `g02.010` modularization completes.

### effigy-catalog (g02.011 — complete, 65 tests)

Crate: `crates/effigy-catalog/`

Modules:

| Module | What it does |
|--------|-------------|
| `schema.rs` | `service.toml` parsing — parameter types, defaults, capabilities, volumes |
| `fragment.rs` | Fragment loading from 3 layers (project-local > user-global > embedded), extract API |
| `template.rs` | Jinja2 rendering via minijinja, parameter validation, context building |
| `assembly.rs` | Compose assembly via serde_yaml — fragment rendering, YAML merge, volume/service wiring |
| `output.rs` | File writing with checksum caching, Docker Compose multi-file args, eject flow |
| `volumes.rs` | Volume classification for reset, Docker CLI command specs for list/export/import |

Bundled fragments (9):

| Fragment | Notes |
|----------|-------|
| `php-fpm` | `install-php-extensions`, Composer, optional Node.js, dev PHP config |
| `nginx` | 4 config variants (default, laravel, spa, wordpress), gzip, security headers |
| `mariadb` | utf8mb4, healthcheck, InnoDB tuning |
| `postgres` | healthcheck, shared_buffers tuning |
| `redis` | Alpine-based, minimal |
| `memcached` | Configurable memory limit |
| `mailpit` | SMTP catch-all with web UI |
| `minio` | S3-compatible storage with persistent volume |
| `elasticsearch` | Single-node, memory-limited, persistent index |

### effigy-gateway (g02.014 + g02.016 — in progress, 88 tests)

Crate: `crates/effigy-gateway/`

Modules:

| Module | What it does |
|--------|-------------|
| `routes.rs` | Route table with atomic JSON persistence, live reload via RwLock |
| `dns.rs` | UDP DNS resolver on hickory-proto, `*.test` A records, AAAA handling, query stats |
| `proxy.rs` | Streaming HTTP/HTTPS proxy, WebSocket, body limits, response timeout, graceful drain |
| `tls.rs` | mkcert certs, rustls config, SNI resolver for multi-domain HTTPS |
| `server.rs` | Coordinated DNS+HTTP+HTTPS+watcher lifecycle, PID files, signal handling |
| `ports.rs` | Port allocation registry with gap filling, conflict detection, service offset map |
| `registration.rs` | Atomic route register/deregister for container lifecycle events |
| `resolver_setup.rs` | macOS `/etc/resolver/` file management |
| `stats.rs` | Atomic request/DNS counters, uptime tracking, JSON serialization |

### effigy-exec (g02.012 — in progress, 70 tests)

Crate: `crates/effigy-exec/`

Modules:

| Module | What it does |
|--------|-------------|
| `routing.rs` | Host vs container decision engine, host-native allowlist, task overrides |
| `cwd.rs` | Bidirectional host↔container path translation |
| `alias.rs` | Named exec aliases with multi-word command support |
| `detection.rs` | Container capability probing, handoff vs raw-exec strategy, capability cache |
| `health.rs` | Health check parsing, polling state machine, ready-state tracking |

### Integration Path

When `g02.010` finishes, these crates wire into the runner:

1. **Manifest schema**: add `services`, `context`, `exec`, `dns`, `data`
   sections to `[containers]` config.
2. **Catalog dispatch**: when a container has `services` instead of
   `compose_file`, call `effigy-catalog` to assemble compose.
3. **Route registration**: on `container up`, call
   `effigy-gateway::registration::register_route`. On `container down`,
   call `deregister_route`.
4. **Exec routing**: before task execution, call
   `effigy-exec::routing::route()` to determine host vs container target.
5. **CLI commands**: add `effigy exec`, `effigy gateway`, `effigy service`
   subcommands.
6. **Port allocation**: when `host.ports` are omitted, allocate from
   `effigy-gateway::ports::PortRegistry`.

### Test Coverage

| Crate | Unit | Integration | Total |
|-------|------|-------------|-------|
| effigy-catalog | 36 | 31 | 67 |
| effigy-gateway | 74 | 14 | 88 |
| effigy-exec | 70 | 0 | 70 |
| **Total** | **180** | **45** | **225** |

## Related Guides

- `docs/guides/063-container-system-guide.md` — v1 container operator guide
- `docs/guides/061-rhai-script-steps-guide.md` — Rhai scripting
- `docs/guides/050-env-schema-integration.md` — environment variable handling
- `docs/guides/012-dev-process-manager-tui.md` — TUI process manager

## Apple Containers 1.2 Reassessment

Apple Containers is a credible optional macOS backend candidate after its 1.2
release, but it is not Compose-compatible. Each container runs in its own
lightweight VM and the CLI supplies the required low-level OCI build,
lifecycle, exec, network, volume, and port primitives. Service-stack
orchestration remains Effigy-owned.

The current catalog-to-generated-Compose flow must therefore evolve into:

```text
manifest + catalog + overrides
             |
             v
    typed effective stack plan
        /                 \
       v                   v
Compose renderer      native operation plan
(Docker/Colima)       (Apple Containers)
```

The typed plan, not Compose YAML or backend command construction, is the
durable semantic boundary. It carries service identity, build/image, command,
environment, mounts, ports, networks, dependencies, readiness, execution
target, gateway metadata, and data lifecycle policy.

The prototype established this stack-plan boundary and a bounded native
executor. Its disposition is watch-only. Direct `compose_file` input and
arbitrary Compose overrides remain Docker/Colima-only, and Apple is not
registered or auto-detected.

The blocking architectural gap is boot-time service discovery. Project-local
host reconciliation works after containers start, but Apple 1.2 supplies no
bare service-name DNS or static address assignment. A service whose boot
process requires a peer alias can therefore fail before Effigy can repair
`/etc/hosts`. Gateway/runtime-prep integration, secret delivery, SSH/Rosetta
policy, and project-scoped data operations also remain incomplete.

Current authority remains the live contracts:

- `docs/contracts/006-compose-backend-compatibility.md`
- `docs/contracts/012-container-manager-contract.md`
- `docs/research/translation-memos/017-apple-containers-runtime-backend.md`

## Next Task

Treat this document as background. Keep Apple Containers watch-only and retain
Docker/Colima as the supported runtime paths. Reopen implementation planning
only when the boot-time discovery boundary or an equivalent safe Effigy repair
has changed; Translation Memo 017 holds the prototype matrix.
