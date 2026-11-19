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
2. Load matching catalog fragments (project > bundle `catalog/` > declared `[catalog.sources]` > compiled baseline).
3. Substitute parameters into fragment templates.
4. Assemble into a complete compose file.
5. Collect generated Compose, Dockerfiles/configs, shared-service renders,
   workspace/DNS rewrites, and user overrides in one in-memory kernel closure.
6. Observe returns the closure's digest and prospective immutable object path;
   it performs no runtime or Git writes. Prepare publishes through kernel
   Objects and the backend receives the generation root for asset references.
7. A changed render creates a new immutable generation. Admission pins the
   desired object reference; reset drops only its environment's references.

The [container runtime contract](../contracts/005-container-runtime-contract.md)
owns rendering, migration, export and reset semantics.

### Catalog distribution

The baseline catalog fragments are **embedded in the Effigy binary**. The
assembler reads the selected layer and contributes Compose and rendered assets
to the in-memory closure. Prepare publishes the complete immutable generation. A consumer
does not need a separate catalog checkout or network access for the baseline.

Catalog sources layer on top, highest priority first (Q-005, architecture
[026](026-feature-placement-and-command-surface.md#service-catalogs)):

1. **Project:** `infra/dev/catalog/` in the repo
2. **Bundle:** the pinned bundle's shipped `catalog/`
3. **Declared sources:** `[catalog.sources.<name>]`, pinned per checkout and
   moved by a host Prepare or `effigy bundle sync`
4. **Baseline:** embedded in the binary (lowest priority)

`effigy service list` shows each service's source and what it shadows.
`effigy service extract <service>` copies a baseline fragment into the project
catalog for customization. The installed catalog-pack layer is retired.

### Repo-owned Compose (retired)

Effigy owns the environment; Compose is the engine adapter (Q-004). Direct
`compose_file` ownership and `eject` are retired. For one release a
`compose_file` environment is read as project-layer fragments under
`infra/dev/catalog/`, and `effigy container migrate <name>` writes them out
with an unchanged render (contract 005). `effigy container render <name>`
shows the exact render.

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

Every runtime read Effigy makes is bounded. A bring-up's reads after Compose
returns are bounded by one deadline: the environment's own declared
`[containers.<NAME>.health].timeout_secs` (60s when it declares none), started
when Compose returned. The restart-policy read that follows the apply spends
that budget first and the readiness wait spends the rest of it, so a runtime
that stops answering an inventory read is reported once, with the readiness
evidence and the probe child stopped and reaped at the deadline, instead of
holding the command open or being answered with a second Compose activation
(effigy#411). A read that names no deadline of its own still gets the standard
runtime read budget, so no inventory read waits out a hung runtime.

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
| `listener_handoff` | launchd socket activation for the root-owned bind broker's Unix socket, plus validation and adoption of the descriptors the broker hands the operator gateway; the direct daemon mode that adopted TCP `127.0.0.1:80`/`443` was retired in v0.16 (see [gateway migration and the launchd handoff ruling](034-gateway-legacy-upgrade-recovery.md)) |
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
`host:port` target and cannot be combined with `service` in one route. Linked
worktrees that do not share runtime identity rewrite the declared apex to
`<apex>-w<host-key>.<tld>` so HTTP routes and TCP aliases stay one set.
Generated TCP aliases and `container hosts` cookie domains use the first
resolved route's domain by default; `containers.<name>.dns.base_domain` can
override that apex without changing HTTP route names.
`effigy container hosts` exposes that map. The primary checkout keeps declared
names. Consumer apps still feed those names into public URLs, origins, cookies
and selectors; see Queue lead `af3ab7c7-b631-4d4f-9da7-f2949ded82f3`
for remaining consumer wiring.

Managed listener transitions keep a separate bounded receipt beside listener
state. The receipt records the supervisor's selected cause and launch
assignments, but does not authorize route, process or cleanup effects. Contract
005 owns its v2 schema, historical v1 reader behavior and incomplete-evidence
limits.

Managed listener startup distinguishes an explicit stop observed while waiting
from a readiness or report failure. That selected outcome is fixed before child
teardown; a later rollback signal cannot turn a failed startup into stopped.
Generic route cleanup preserves failed status and its bounded phase diagnostic.
An incomplete pre-publication cleanup retains the exact child generation as
unavailable, rather than claiming that startup continues or teardown succeeded.
Supervision admission and listener readiness are separate phases: the supervisor
publishes this activation's `starting` record before it waits on dependencies or
spawns a child, so a slow supervisor start or a slow activation publication is
supervision admission, never readiness. Readiness itself has two phases: while
the reserved port is closed and the child is alive the wait has no deadline, and
the declared readiness budget starts when the port first answers. The caller
waits on the supervisor's own record rather than a deadline of its own, so its
refusal keeps the supervisor's phase diagnostic, and a supervisor that exited
without publishing readiness is the failure it names.

For a directly spawned child, the supervisor observes kernel exit state without
reaping the leader. It stops surviving group members before reaping that leader
and returns the real child exit status. The unreaped child reserves its PID
through TERM, the existing grace period and bounded KILL settlement. This applies to directly owned readiness/stop cleanup, including a leader that
has already exited but remains unreaped. Ordinary natural-exit selection still
records the actual exit before state or route effects; surviving descendants
after that reap keep the generation held and prevent automatic restart. A bounded system process-table observation
is completion evidence, not identity or signal authority. Recorded-process
recovery still refuses absent, changed or uncertain leader identities when group
members remain; it cannot borrow the supervisor's owned Child capability.
Private public-CLI and native descendant controls cover this boundary. Real
Linux runtime and remote container cancellation remain separate unverified
limits; no consumer qualification exception follows from these controls.

Private loopback fixtures that read synchronously normalize accepted sockets to
blocking mode while retaining their existing read timeouts. Acceptance alone
does not prove request bytes are ready; a foreign-listener sentinel must not
lose its positive isolation probe to inherited nonblocking mode.

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

The gateway PID file remains a decimal `u32`, but reads accept only values
greater than 1 that fit the signed Unix PID type. Status probes require `ps`
to return exactly that PID, and stop/elevation paths validate the target again
before probing or signaling. This prevents PID 0 and out-of-range values from
becoming process-group or broadcast targets.

A gateway process probe is three-valued: a single exact non-zombie `ps` row is
running, a zombie row or the `ps` no-such-process result is confirmed absent,
and a `ps` launch failure, a failed `ps` carrying a diagnostic, or empty,
malformed, mismatched or multiple rows is unknown. `effigy gateway status`,
`up` and `down` and daemon start consume that value; unknown is never reported
as stopped, never deletes the PID/version records and never starts a
replacement daemon. Status and unlocked `up` preflight are read-only:
confirmed absence and a readable identity mismatch leave records in place.
Authenticated stale-record cleanup runs after `gateway.transition.lock`, at
daemon start (`check_existing_gateway_pid`) or on stop compare-and-remove.

Those checks prove numeric domain and current liveness of some process. They
do not prove that the live process is the gateway that wrote the file. See
[Gateway process identity](#gateway-process-identity).

### Gateway process identity

The gateway records a persisted control record beside its routes: the daemon
PID, the boot identity and the precise process start identity. A process is the
gateway only when all three match the record; only then may the lifecycle treat
it as running or send it a signal. A missing, unreadable or legacy record is
`Unknown`: the record is preserved byte-identical, nothing is signalled and no
replacement starts. A numeric PID alone never authorizes a signal.

- **Boot identity.** macOS reads the kernel `kern.bootsessionuuid` session
  identity through `sysctl` and accepts only a canonical 8-4-4-4-12 UUID;
  missing, failed, empty, non-UTF-8 or malformed output is `Unknown`. Linux
  reads `/proc/sys/kernel/random/boot_id`. `effigy_process::compare_boot_identity`
  is the single comparison point and answers `Match`, `DifferentSession` or
  `Unknown`; a record written before the change holds the `kern.boottime`
  timeval, whose seconds component is wall-clock-adjusted within one boot, so a
  changed or unreadable legacy value is `Unknown`, never `DifferentSession`.
  `probe_live_identity` uses that comparison for the ordinary and elevated
  readers alike.
- **Cross-user access.** Apple applies `CHECK_SAME_USER` to
  `PROC_PIDTBSDINFO`, so an ordinary operator client cannot read a root-owned
  daemon's live identity. Q-002's bounded read-only identity command runs
  through Effigy's existing administrator elevation, reads only the trusted
  recorded target, returns bounded identity/status data and never signals,
  changes records or starts anything. A declined or non-interactive prompt is
  `Unknown`, and the lifecycle then refuses. No new helper, standing privilege
  or wider privilege model follows.
- **Legacy records.** The supported upgrade starts from v0.14.1's
  identity-bearing state; a numeric-only record requires the administrator
  migration path in
  [architecture 034](034-gateway-legacy-upgrade-recovery.md#migration-boundary),
  not a numeric guess.
- **Residual limit.** The last identity comparison and the TERM/KILL syscall are
  separate operations, so a PID can be reused in that interval. No pidfd or
  atomic targeting is claimed.

The daemon registers Unix TERM and INT handling before publishing its
generation, forwards the shutdown signal to its server tasks, and joins every
owned DNS, HTTP, HTTPS and TCP-alias task before removing the exact unchanged
identity record. Bounded waits: the read-only identity reader 15 seconds, a
full elevated lifecycle 30 seconds, managed auto-start 35 seconds and printing
the required operator action before it waits. A timeout reaps only the owned
launcher and reports the gateway outcome as unknown; the caller neither guesses
that an elevated child finished nor removes records. Run `effigy gateway
status` before retrying, and use `effigy dev status` / `effigy dev stop` before
the next `effigy dev --headless` invocation for a task-local development
restart.

### Gateway reuse, attribution and private state

A managed start uses the running gateway, warns when its build differs, and
starts one only after positive absence. An unavailable observation warns and
proceeds without starting a second daemon. Replacement belongs to explicit
`gateway down` / `gateway up`. Debug fixture commands require a private root,
including down and platform setup, and operator lifecycle tests refuse while
the machine ports are occupied; a private HOME alone does not isolate the
broker.

Command stops and kernel custody settlement append the caller's pid, argv, cwd
and cause to `gateway.stderr.log`; a custody stop can follow owner loss, a
durable stop request, cancellation or interrupted-run recovery even when no
`gateway down` ran. The `gateway_setup`, `up` and `down` event carries a bounded
`caller` record: start identity, session and controlling terminal of the
invoking process and its parent chain to the session leader (at most eight
entries), plus only allowlisted `PASEO_*`, `NS_*` and `EFFIGY_RUN_*` markers
whose value is a supported identity form, recorded as that form plus a SHA-256
fingerprint. The controlling terminal comes from the kernel's own record for
the session (`bsdinfo` on macOS, `/proc/<pid>/stat` on Linux), never from a
descriptor or `/dev/tty`. Command text is published by role: the invoking
program plus Effigy's own flag names, every other token `<redacted>`, and only a
program name for an ancestor. An unreadable parent, argument or marker, a
changed start identity, a parent in another session, or an omitted reason is a
bounded unknown reason, never a guess. The event is written at the command
boundary only, a task run that starts the gateway in-process records a managed
start, and logging never authorizes, refuses or delays an action. The elevated
child of `gateway setup` writes nothing.

On macOS, gateway setup manages `/etc/resolver/` files for local domains. HTTPS
uses mkcert-backed certificates after `effigy gateway setup-tls`; plain HTTP
redirects to HTTPS for TLS routes. `.test` names work without TLS; a `.dev`
route needs certificate setup before it is useful. The gateway serves
certificates, mkcert manages the local trust chain, and route lifecycle stays
tied to the owning environment. DNS-only TCP aliases for services such as
Postgres, MariaDB, Redis and Memcached use deterministic loopback targets, so
no manual `/etc/hosts` edit is needed. The gateway can serve non-container host
listeners when the manifest explicitly declares them.

A disposable consumer can select a caller-owned private gateway with
`effigy gateway setup-tls --private-state-root <DIR>` and
`effigy gateway up --private-state-root <DIR>`, or
`EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=<DIR>` for normal route registration and
managed startup. The existing root must be a canonical owner-only directory;
private mode keeps routes, process records, listeners, certificates and the
mkcert CA under it, binds DNS, HTTP and HTTPS only to loopback on unprivileged
ports (port `0` requests actual OS allocation), and reports the sockets its
generation holds. It installs no CA trust, modifies no resolver file,
provisions no host loopback alias and requests no elevation. An absolute
`EFFIGY_GATEWAY_MKCERT_BIN` selects mkcert; otherwise Effigy searches its
bounded trusted install prefixes, and an invalid explicit path fails closed.
`<DIR>/ca` is passed as `CAROOT` and only certificate generation runs, never
trust installation; clients trust `<DIR>/ca/rootCA.pem` and resolve each route
name to the reported HTTPS address. Non-Unix platforms fail the private-root
validation closed.

A checkout's loopback alias is not a separate record: the gateway's route table
is the only record of loopback ownership, so an address is in use exactly when
some route references it as its `dns_ip` or as its target. `up` takes the
address the checkout's routes already answer with, or the lowest address in the
`127.1.0.1`-`127.1.0.250` pool that no route references; `down` and retirement
free it by removing those routes, and any route write drops the routes of a
checkout path that is missing. A deleted worktree therefore keeps neither a
domain nor an address, and no cross-runtime inventory or reclamation plan
exists to run.

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
`data:seed` task. It currently targets the default container. The
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
missing or ambiguous targets. Secret values stay out of reports, and a
credential travels by name only
([contract 005](../contracts/005-container-runtime-contract.md#shared-data-targets-data-seed-data-dump)).

Cache and persistent volume cleanup are separate. Generated Compose can put
Rust `target`, `node_modules`, and pnpm store paths on disposable named
volumes. `container cache list/prune` classifies those build caches;
`container volume list/prune` handles named volume ownership, dormant repo
volumes, and global orphans. The global cache prune deletes only volumes
whose own Effigy labels prove ownership (`com.effigy.managed`,
`com.effigy.project`, `com.effigy.volume-name`) whose labelled project is
not running; name and content classification stay diagnostic, lookalikes and
unlabelled legacy volumes are reported and left intact, and failed listing,
inspection or discovery is an error, never absence. Persistent application
data is never swept as a dormant or orphan cache. Explicit destructive
operations require confirmation.

```sh
effigy container cache list --global
effigy container cache prune --kind rust-target --yes
effigy container volume list --dormant
effigy container volume list --global --orphans
effigy container volume prune --global --orphans --yes
```

Cache classification uses mount targets as well as names so older opaque
`efv-*` volumes can still be recognized in the read-only inventory; it never
authorizes deletion. A named volume under an application data path is never
reclassified as disposable only because it is idle.

External host mounts use structured tables with `external = true`; legacy
string mounts remain repo-relative. Generated published ports bind to
`127.0.0.1` by default; a container may opt into `publish_address =
"0.0.0.0"`. These defaults protect local databases and admin endpoints from
unintentional LAN exposure.

## 6. Multi-Project Coordination

Host ports come from the OS or Compose: a generated service publishes
`127.0.0.1::<container-port>`, and `up` reads the assigned port back from the
running container into its route. Several environments therefore run together
without hand-assigned ports and without a machine-wide registry. A manifest pin
in `[containers.<NAME>.host].ports` is used as written and a collision names the
process holding it; `[containers.<NAME>.host].stable_ports = true` derives an
unrecorded port from the checkout path instead, without probing the runtime.

`container status --global`,
`container stats --global`, `container cache list --global`, and
`container volume list --global` expose machine-wide state; gateway status
shows registered routes.

A generated service may declare `shared = true` for bounded shared backing
services. The policy rejects unsupported variant/config combinations and
requires at least one local service. Sharing trades isolation for resource
use; it is explicit, not an automatic optimization.

Gateway domains are globally keyed by hostname. Linked worktrees receive
distinct effective names from the declared apex, so two live stacks can
register together. `effigy container retire` removes one scope's owned
resources by label and recorded identity without a global prune, and only
on the backend that produced each observation. Shared
routes stay when a worktree mixes isolated and shared-identity stacks;
repo-owned Compose volumes stay unless labelled mutable.

## Crate and Module Ownership

| Owner | Role |
| --- | --- |
| `effigy-catalog` | fragment schema, layered lookup, template rendering, compose assembly, volume classification and identity, catalog sources and fragment requirements |
| `effigy-manifest` | systems, workspaces, containers, DNS routes, services, lifecycle, host, data, and alias grammar |
| `effigy-containers` | effective policy, generated Compose, manager facade, backend operations, workspace transport |
| `effigy-exec` | host/container route decision, cwd translation, aliases, capability detection, readiness model |
| `effigy-runtime-plan` | typed activation stages and policy |
| `effigy-runtime` | runtime read/write/data/shell adapters |
| `effigy-data` | database target and seed/dump domain plans |
| `effigy-gateway` | DNS, proxy, TLS, route registration, loopback aliases, TCP aliases |
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
provider topology. A repository that needs its own service definition writes
a project catalog fragment; the retired `compose_file` translates to one.
Release and provider export remain separate surfaces.

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
