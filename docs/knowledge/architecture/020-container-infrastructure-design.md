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
| `listener_handoff` | launchd socket-activation adoption of TCP `127.0.0.1:80`/`443` for the operator-owned gateway — the internal `--listener-handoff` opt-in; v0.15 retired the experimental definition generator and inspection/removal commands, and live activation and cross-boot proof stay operator-owned (see [gateway migration and the launchd handoff ruling](034-gateway-legacy-upgrade-recovery.md)) |
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

This subsection is the owner for current behavior, source evidence, and the
bounded correction history. Tom's ruling lives in
[Gateway process identity ruling](#gateway-process-identity-ruling).

#### Behavior before task 104 (main `127291bac`, verified at dispatch)

Persisted control record (`crates/effigy-gateway/src/server.rs`):

- `write_pid_file` writes `std::process::id()` as decimal text. No owner-only
  mode, no atomic publish, no boot id, no start identity, no comm/exe/uid.
  The writer is the daemon after start. When `gateway up` elevates
  (osascript/sudo), that process has euid 0 and writes into
  `~/.effigy/gateway`, which `prepare_gateway_state_for_elevated_run`
  created as the unelevated operator. Root `std::fs::write` plus a typical
  umask leaves a root-owned world-readable file, so unelevated
  `read_pid_file` still works. `routes.json` `0o600` is written by the
  unelevated CLI; that ownership does not apply to a root-written sidecar.
- `gateway.version` beside it records the daemon binary version only.
- `read_pid_file` parses `u32` then `checked_gateway_pid` (reject 0, 1, and
  values that do not fit positive `pid_t`). Permission-denied reads surface
  as I/O errors through `get_status`, not as a dedicated unknown.

Liveness (`probe_gateway_process`): Unix `ps -p <pid> -o pid= -o stat=`, one
exact non-zombie row → `Running`. No command, uid, start time, or boot id.
Non-Unix reports a valid-domain PID as `Running` with no probe.

Lifecycle consumers:

- `get_status` / `check_existing_gateway_pid`: `Running` means the gateway is
  up (`AlreadyRunning` on start). `ConfirmedAbsent` clears the PID and version
  files. `Unknown` leaves them byte-identical and returns
  `ProcessStateUnknown`.
- `run_gateway_status` / `gateway_up_for_managed_task` use that status.
- `handle_existing_gateway_for_up`: matching binary version returns
  already-running; otherwise it may elevate, then
  `stop_gateway_process(status.pid)` and start a replacement.
- `run_gateway_down`: `stop_gateway_process(running.pid)` then
  `remove_pid_file` after a confirmed-absent re-probe.
- `stop_gateway_process` (`src/runner/gateway_command/daemon.rs`): domain
  check, refuse the caller PID, then SIGTERM, wait, SIGKILL. Each step
  re-probes liveness only.
- Elevation (`process_signal_accessible`): `kill(pid, 0)` after the same
  domain check. A `false` result asks for elevation (fail-closed for
  privilege). It does not prove identity. An unsignalable leftover PID after
  reboot therefore elevates, then the privileged path can TERM/KILL whatever
  now holds that number.

`kill` takes only integer arguments, so a negative `pid_t` is memory-safe.
v0.13.1's unchecked `u32 as i32` could form `-1` (broadcast) or `0` (process
group): that is numeric-domain / signaling-authority, not a libc memory-safety
hole. 099 closed that domain (`checked_gateway_pid` plus
`gateway_signal_target_is_safe`). Ownership remains open.

#### Baseline v0.13.1 (`08e17227021778b5126adfe06d8a141b05776d70`)

Unchanged across the baseline and HEAD: decimal PID only, `ps` liveness, no
start identity, `stop` signals that PID, elevation `kill(pid, 0)` without
identity. v0.13.1 also used an unchecked `pid as i32` for TERM/KILL and
`kill(pid as i32, 0)`, and collapsed any failed `ps` to not-running (then
deleted the PID file and could start a replacement).

099 (`3991ecbc1`) closed the numeric domain. 101 (`3dea18c91`) closed the
unknown-probe collapse. Ownership is the same as v0.13.1.

#### Historical source counterexample (recording-only)

A live `sleep` child whose decimal PID is written to `gateway.pid` is
`GatewayProcessProbe::Running`. `get_status` returns that PID as the gateway;
`check_existing_gateway_pid` returns `AlreadyRunning`. The probe never reads
comm, uid, boot id, or start identity. Prove with
The pre-task-104 control was named
`server_probe_state_live_non_gateway_pid_is_reported_running`; the identity
selector replaces it with `gateway_identity_matching_private_child_is_reported_running`,
which writes a precise record for its owned fixture before invoking status.
Neither status proof signals the fixture.

The same PID, once treated as running, is the argument to
`stop_gateway_process` on `down` and on `up` replacement when
`gateway.version` does not match the current binary. The in-tree private-child
stop test `gateway_pid_domain_stops_exact_private_owned_child` showed TERM of
a private child given only its PID before task 104.

Static sequence for a leftover file after crash or reboot, no live
exploitation claimed:

1. Daemon writes `N` and later dies without unlinking the file.
2. The kernel reuses `N` for an unrelated process (common after reboot).
3. `get_status` sees one `ps` row for `N` → running gateway.
4. `gateway down`, or `gateway up` with a mismatched version file, sends
   SIGTERM then SIGKILL to `N`. If `kill(N, 0)` is `EPERM`, elevation runs
   first and the privileged path can signal a process the unprivileged user
   could not.

A second, narrower race remains even for a true gateway PID: the process can
exit and be reused between the last `Running` probe and `kill`. A pre-signal
start-time check would still be a TOCTOU, not an atomic signal.

uid/`ps` row matching would not close this. The live foreign process can share
uid with the operator, and an exact PID row is what the probe already
requires.

#### Platform identity primitives (pre-correction inventory)

Historical baseline before the start-identity correction: these primitives were
already in the product but were not used to establish gateway ownership.
The implemented policy is described in the following subsection.

| Primitive | Where | Precision | Gateway use |
| --- | --- | --- | --- |
| `effigy_process::boot_identity` | Linux `/proc/sys/kernel/random/boot_id`; macOS `sysctl kern.boottime` | Per boot | None |
| `effigy_process::process_start_identity` | Linux `/proc/<pid>/stat` field 22 (ticks); macOS `ps -o lstart=` | Linux: tick-granularity within a boot. macOS `lstart` is locale-dependent and must not be persisted as a generation key | QA-group owner liveness (`qa_group_status.rs`); not gateway |
| `effigy_host_run::canonical_start_identity` (retired with the host-run client, effigy#227) | Linux `{pid}@{boot}:{ticks}`; macOS `{pid}@UTC-whole-seconds` from `pbi_start_tvsec` only | The retired host-run wire format truncated macOS start time to whole seconds; never a gateway record | None (removed) |
| macOS `libc::proc_bsdinfo` | `proc_pidinfo(PROC_PIDTBSDINFO)` → `pbi_start_tvsec` and `pbi_start_tvusec` (`u64` each). Locked `libc` 0.2.189 and XNU's public `sys/proc_info.h` both expose the usec field. XNU copies both fields from the kernel process start timestamp. Host-run ignores usec. XNU `proc_info.c` applies `CHECK_SAME_USER`; a cross-uid read needs `PRIV_GLOBAL_PROC_INFO` | Strongest macOS start-time primitive in this tree: pid + boot + sec + usec. Whole-second collision is avoidable truncation. A root-owned gateway is unverifiable through this API from the ordinary operator uid ([Q-002](../questions.md#q-002--macos-gateway-cross-user-identity-access)) | Unused by gateway |
| Linux pidfd | not used | Would make open-then-signal atomic on Linux 5.3+ | Not portable to macOS; out of the smallest fix |

At that baseline, unknown identities already failed closed in host-run and in
`process_start_identity_matches`; gateway had no equivalent match step.

#### Implemented correction in task 104

Published v0.14.0 upgrade/start defect (present in the `v0.14.0` tag, corrected
on main by task 113, `4201e0b3`): that binary's `trusted_directory_owner`
accepted only a directory owned by root or the effective caller UID. During
existing administrator elevation the effective UID is root, so a genuine
ordinary-operator-owned gateway directory was rejected before startup with
`gateway directory is unsafe`, even though the operator UID was forwarded.
The root/operator trust boundary is corrected on main: the elevated root caller
accepts the forwarded non-root operator UID only when it matches ambient
`SUDO_UID` (when present) and owns the forwarded `HOME` per passwd, and
unauthenticated root still accepts only root ownership. Changing directory
ownership or relaxing trust for every owner is not a supported workaround. A
legacy gateway also needs the explicit transition below. A previously installed
binary may have been overwritten, and using an older CLI for development may
fail to parse newer linked manifests; a general CLI downgrade is not a recovery
strategy. The supported upgrade/recovery design is
[034](034-gateway-legacy-upgrade-recovery.md). Published v0.14.0 cannot complete
a pre-0.14 upgrade. Such numeric-only records now require administrator migration; see [architecture 034](034-gateway-legacy-upgrade-recovery.md).

The decimal `gateway.pid` remains compatible. `gateway.identity` is a
version-1 JSON sidecar containing `format_version`, the same `pid`,
`boot_identity`, and a tagged platform start identity. Linux stores `/proc` stat
field 22 ticks; macOS stores `pbi_start_tvsec` and `pbi_start_tvusec` separately.
The sidecar and PID are staged as owner-only `0o600` files and atomically
renamed, sidecar first. A lock file serializes publishers and compare-and-remove.
An identity-only interrupted pair, mismatched pair, malformed sidecar, unsafe
mode, or symlink is unknown and preserved. A root writer validates the gateway
directory owner before `fchown` on the open published file; both record files
are assigned to that directory owner so the ordinary operator can read them.
The decimal file stays owner-only as well. Task 113 extends that owner check
across elevation: an elevated root caller behind the existing
`EFFIGY_GATEWAY_ESCALATED` marker additionally accepts a gateway directory
owned by the authenticated operator UID from `EFFIGY_GATEWAY_OPERATOR_UID`.
That UID is authenticated, not merely read: it must be non-root, match an
ambient `SUDO_UID` when one is present, and name the UID whose passwd home is
byte-identical to the forwarded `HOME`. An unauthenticated root still accepts
only root ownership, so the v0.14.0 `gateway directory is unsafe` rejection of
the genuine operator-owned directory no longer recurs for the authenticated
context, while a forged, substituted, or unrelated owner still refuses. The
spawned daemon inherits the same forwarded `HOME`, escalation marker, and
operator UID, so its publication carries the identical context.

All production lifecycle paths carry `VerifiedGatewayStatus`, which keeps the
unchanged public `GatewayStatus` output paired with the exact trusted record
snapshot. `get_status` and `check_existing_gateway_pid` require the recorded
PID, boot id and precise start identity to match. `get_status` is read-only: a
readable different live generation is not Running, and confirmed absence is
NotRunning, without deleting records. Stale authenticated records are removed
only by byte-for-byte compare-and-remove after the transition lock, including
`check_existing_gateway_pid` at daemon start. A readable PID-only record with
no `gateway.identity` sidecar receives a specific legacy identity migration
error after a read-only process probe. An unknown process probe keeps the
distinct `ProcessStateUnknown` error; a running or confirmed-absent numeric-only
record still preserves its files and refuses signal, cleanup, or replacement.
A present but malformed, PID-mismatched, unreadable or otherwise untrusted
identity also remains unknown and is preserved.
Before TERM and again before KILL, stop rechecks the same record and live
identity; `process_signal_accessible` performs the same check before its
signal-zero permission probe. The `gateway up` check and record publication
share the sidecar policy and a locked second check prevents overwriting a pair
published by another starter.

When the local kernel denies a live-identity read, the runner may invoke the
hidden `__gateway-identity` command through existing `/usr/bin/sudo` or
`/usr/bin/osascript` elevation. It accepts only a digest of the validated
record, its directory owner UID, and a digest of the canonical fixed gateway
PID path; it accepts no PID or path. The root command verifies the elevation
marker and operator UID, rechecks the canonical path and trusted byte pair,
reads only that recorded target, and returns at most 1 KiB of identity state.
It never signals, changes records or starts a daemon. The caller requires an
interactive terminal, caps the reader wait at 15 seconds, and rejects
nonzero exit, timeout, malformed output, or either digest mismatch as Unknown.
Declined, unavailable and non-interactive authentication therefore preserve
the lifecycle's fail-closed behavior. The reader's ordinary private tests do
not substitute for a live cross-UID integration proof.

The production caller chain is `run_gateway_status` and
`gateway_up_for_managed_task` → `verified_gateway_status` →
`server::get_verified_gateway_status_with`; `run_gateway_up` first passes the
same snapshot through `handle_existing_gateway_for_up`, and daemon start uses
`check_existing_gateway_pid` plus locked publication. `run_gateway_down` and
version-mismatch `up` pass the retained snapshot to `stop_gateway_process`;
`gateway_down_requires_elevation` / `process_signal_accessible` validate it
before selecting the elevated down path. Daemon shutdown removes only the
snapshot it published. No production caller uses a numeric-only signal path.

Legacy records remain untouched and block `status`, `up`, `down`, and managed
start. A missing sidecar is reported as a migration requirement rather than a
process-probe failure; if the process probe itself is unknown, that unknown
remains distinct. Version-only, malformed, or symlink `gateway.version` state
is the same fail-closed unknown: the shared classifier refuses before start
can overwrite those files, and version publication refuses to follow a
symlink target. The diagnostic does not assume the previous executable is
still installed and does not authorize PID-only signaling. A pre-0.14
numeric-only record requires administrator migration: verify and stop that
exact daemon, confirm absence, and remove only its old PID/version records.
Unknown identity preserves all records and refuses replacement; see the
[migration boundary](034-gateway-legacy-upgrade-recovery.md#migration-boundary).
A v0.13.1 rollback ignores `gateway.identity` and restores the PID-only risk;
downgrade is not claimed safe. Host-run
contract 010 and its whole-second wire identity are unchanged. Non-Unix
`down` remains unimplemented.

Private controls cover exact-byte preservation for legacy/malformed and
interrupted pairs, unsafe/symlink records, owner-only publication, target/path
substitution, boot/start mismatch (including same-second/different-usec macOS
identity), compare-and-remove against a newer pair, bounded reader parsing and
timeout, declined/non-interactive reader results, and recording-only zero
signal dispatch for mismatch/unknown identity. The existing private owned
child TERM-resistant cleanup control remains the positive signal-path proof.
The supported API evidence remains `libc::proc_bsdinfo` plus the SDK's
`sys/proc_info.h`. The public [XNU header](https://github.com/apple/darwin-xnu/blob/main/bsd/sys/proc_info.h#L2286-L2331)
declares the two start-time fields, [the libproc header](https://github.com/apple-oss-distributions/xnu/blob/main/libsyscall/wrappers/libproc/libproc.h#L695)
declares `proc_pidinfo`, and [the XNU kernel implementation](https://github.com/apple/darwin-xnu/blob/main/bsd/kern/proc_info.c#L3316-L3360)
fills both fields from `p_start`. An actual cross-UID kernel read cannot be exercised without
the prohibited live elevated daemon, so the implementation discloses that
integration limitation rather than claiming it was run.

#### Follow-up correction in task 117

The task-104 correction left two defects that surfaced on the latest bootstrap
against a live root gateway:

- `gateway_up_for_managed_task` ran its `sh -lc` child through
  `ProcessCommand::output()`, which replaces stdin with `null`. The bounded
  read-only elevated identity reader requires `stdin.is_terminal()`, so a
  managed auto-start against a root-owned live gateway could not read the
  cross-UID generation and failed with an Unknown "cannot determine gateway
  state" error even though the daemon was healthy. The managed transport now
  spawns the child with `Stdio::inherit()` on stdin and piped stdout/stderr,
  then drains both pipes with `wait_with_output`. Noninteractive and declined
  authentication still return Unknown, so the lifecycle refuses without a
  signal, cleanup, or replacement. Startup text is state-keyed (`stopped` or
  `unverified`); an unknown or mismatched live daemon is never described as
  "down". A managed start that finds any running gateway uses it instead of
  starting another (see "Managed start never replaces a running gateway"
  below).
- The macOS boot identity was the raw `kern.boottime` timeval, whose
  microsecond field is not stable across reads. A sidecar written by the
  published v0.14.0 macOS build could therefore compare `Mismatch` against the
  live process it had itself published. `effigy_process::boot_identity` now
  reads the kernel `kern.bootsessionuuid` session identity through `sysctl`,
  requires a successful command status, and accepts only a canonical
  8-4-4-4-12 UUID; missing, failed, empty, non-UTF-8, or malformed output is
  Unknown. Linux is unchanged (`/proc/sys/kernel/random/boot_id`).

`effigy_process::compare_boot_identity` is the single comparison point and
returns three values: `Match`, `DifferentSession`, or `Unknown`. New records
store the session UUID: an equal value is a match, and a different canonical
session identity proves another boot. Records written before the change store
the `kern.boottime` timeval. Its seconds component is wall-clock-adjusted
within one boot (XNU adjusts the boot-time value on a calendar change), so a
changed or unreadable legacy value is `Unknown`, never `DifferentSession`: the
ordinary lifecycle preserves the record and refuses instead of deleting a live
daemon. A parse must see the braced timeval with exact `sec` and `usec` decimal
fields; malformed input is `Unknown`. The exact process start identity stays
mandatory for every match, so a reused PID never matches. No old sidecar is
reclassified as absent or rewritten. `probe_live_identity` uses this
comparison for both the ordinary and elevated readers; `Unknown` means the
record is preserved and `status`/`up`/`down`/managed start refuse. A live
pre-identity daemon requires the host administrator migration
path. An ambiguous identity-bearing record is honestly refused, not guessed at.
There is no automatic cleanup or replacement path for that case; preserve the
records until the administrator can establish the exact generation. The
retained v0.14.1 lifecycle and the retired pre-identity window are distinguished
in [architecture 034](034-gateway-legacy-upgrade-recovery.md#migration-boundary).

Shared boot-ID callers are unaffected on macOS: the retired host-run client
derived its wire identity from `pbi_start_tvsec` only, and the QA-group owner check uses
`process_start_identity`, not `boot_identity`. The QA-group record's
informational `boot_identity` field now carries the session UUID on macOS.

Private controls added by task 117: `effigy-process`
`boot_session_uuid_parser_rejects_missing_failed_and_malformed`,
`boot_session_identity_reader_is_stable_and_ignores_boot_time`,
`legacy_boot_time_identity_matches_across_microsecond_drift`,
`legacy_boot_time_change_is_unknown_not_a_different_session`,
`legacy_boot_time_parser_rejects_malformed_fields`,
`different_session_identity_is_known_different_and_malformed_is_unknown`, and
the separate-process `boot_identity_uncached_is_stable_across_separate_processes`;
`effigy-gateway`
`gateway_identity_legacy_boot_time_record_matches_across_microsecond_drift`,
the full-path
`gateway_identity_legacy_boot_time_sidecar_stays_running_across_drift`, and the
preservation control
`gateway_identity_ambiguous_legacy_boot_time_preserves_live_record`;
`effigy` `gateway_up_for_managed_task_preserves_terminal_stdin_and_diagnostics`
(real private PTY, null-stdin diagnostics, and a null-stdin unknown-identity
case proving the managed start never launches) and
`gateway_up_for_managed_task_startup_notice_is_state_accurate`. Maintained
selectors: `test:gateway:boot-identity`, `check:gateway:boot-identity`,
`test:gateway:managed-tty`.

Residual: the last identity comparison and TERM/KILL syscall are separate
operations. A process can exit and its PID can be reused in that interval; this
bounded portable fix does not claim atomic process targeting or add pidfd.

#### Gateway reuse and fixture isolation

A managed start uses the running gateway, warns when its build differs, and
starts one only after positive absence. Unavailable observations warn and
proceed without starting a second daemon. Replacement belongs to explicit
`gateway down` / `gateway up`. Debug fixture commands require a private root,
including down and platform setup. Operator lifecycle tests refuse while the
machine ports are occupied; a private HOME alone does not isolate the broker.
Command stops and kernel custody settlement append the caller's pid, argv, cwd
and cause to `gateway.stderr.log`. A custody stop can result from owner loss,
a durable stop request, cancellation or interrupted-run recovery, even when no
`gateway down` command ran. This identifies possible causes of the 2026-10-26
clean exits, without attributing an unobserved incident to one of them.

The 2026-10-28 15:26:25Z stop of the installed Mac gateway showed what those
fields could not answer: which terminal, session, agent or task asked for it.
The same event now carries a bounded `caller` record for `up`, `down` and
`setup`, and each action names itself when it starts, so a refusal or an action
with nothing to do is attributable as well: the start identity, session and
controlling terminal of the invoking process, its parent chain up to and
including the session leader (at most eight entries, each with pid, start
identity, session and redacted command), and only allowlisted `PASEO_*`, `NS_*`
and `EFFIGY_RUN_*` identity markers whose value is a supported identity form. An
unreadable parent, argument or marker, a pid whose start identity changed or
could not be confirmed by a second read, or a parent in another session —
a reparented child ends the walk instead of naming another session's leader —
becomes a bounded unknown reason instead of a guess, and no environment value is
read unless its name is on that allowlist. The controlling terminal is the
session's terminal, resolved from the kernel's record of it by device on each
platform (`bsdinfo` on macOS, the `/proc/<pid>/stat` line on Linux) and never
taken from a standard descriptor or named by the generic `/dev/tty` alias, so a
redirected descriptor or one attached to another terminal never misattributes the
action. The reason list keeps chain failures ahead of marker failures and
ends with how many reasons it omitted, so a full list never hides a missing or
unconfirmed part of the chain.

Command text is published by role, not by spelling. The invoking process's own
`argv` records the program's file name and one of Effigy's own flag names; every
other token is `<redacted>` — a positional argument whatever its spelling, a
path, an unknown flag, a URL, a `KEY=value` pair, a JSON body, and the value of
any flag in `--flag value` or `--flag=value` form, so `gateway setup --suffix
down` records `--suffix <redacted>`. An ancestor's command is another program's
text, so only its program is published — the executable the operating system
reports, never its caller-chosen `argv[0]` — and no option token of it is, even
one that spells an Effigy flag; a shell command string is foreign text too and
publishes nothing inside itself. A marker is read only when its value is a UUID, a decimal
id, or Effigy's own base32 run id for `EFFIGY_RUN_*`, and it is recorded as that
form plus a SHA-256 fingerprint, never as the value: an ID-shaped value can still
be a credential, and a password pasted into a marker variable is refused by name
only. A stop signalled by replacement or
migration is attributable through the `up` or `setup` action that requested it;
no lower-level helper writes a second record for one action. The event is written
at the command boundary only, so a `setup` step that starts or stops the gateway
emits its single `gateway_setup` event, and a task run that starts the gateway
in-process is recorded as `managed gateway start`, never as an operator
`gateway up`. Logging
stays best effort and never authorizes, refuses or delays an action; the
elevated child of `gateway setup` writes nothing, because the file would become
root-owned in the operator's directory.

### Gateway process identity ruling

Tom ruled on 2026-10-06: "Approved, but you need to fix this so other users
don't have the same problem." This approves the proposed one-time transition
of his existing legacy gateway: use a verified previous binary to stop the
confirmed daemon, confirm it is gone before reconciling only its PID/version
records, then start the current gateway. Routes, certificates, containers and
volumes are preserved. This is explicit authority for that host transition,
not a general exception allowing numeric-only records to authorize signals.
That historical pre-0.14 recovery window is retired in v0.15. The supported
upgrade starts from v0.14.1 identity-bearing state; older numeric-only records
require administrator migration. The earlier candidate-inspection and stop
protocol is no longer an implemented surface. See the current
[migration boundary](034-gateway-legacy-upgrade-recovery.md#migration-boundary).
This historical one-host ruling is not standing authority to signal a PID-only
record.

Tom ruled on 2026-10-05: block v0.14.0 publication until a persisted
start-identity sidecar and fail-closed legacy policy are implemented. He does
not accept the current PID-only ownership risk and authorized the bounded fix.

The correction must match the recorded PID, boot identity and precise process
start identity before treating a process as the gateway or signaling it.
Use Linux boot identity plus start ticks; on macOS retain both start seconds
and microseconds. Missing or unreadable identity is unknown: preserve records,
do not signal, and do not start a replacement. A numeric-only legacy record
must not authorize a signal. Migration must confirm the exact owned daemon
stopped/gone before removing its records.

The legitimate unelevated reader must be able to read the trusted sidecar
published by an elevated daemon. Private proofs must establish that read policy
and live identity access. Tom's 2026-10-06 Q-002 ruling authorizes only the
bounded read-only command through existing administrator elevation described
below; no new helper or host-run contract change is authorized. A last identity check still races the signal syscall;
implementation and release documentation must disclose that limit rather than
claim atomic ownership. No live gateway migration or release publication is
part of this authorization.

The macOS supported-reader constraint is now established: Apple's XNU
`proc_info.c` applies `CHECK_SAME_USER` to `PROC_PIDTBSDINFO`; a cross-uid
read requires `PRIV_GLOBAL_PROC_INFO`. The existing root-owned gateway is
therefore unverifiable through this API by an ordinary operator client.
A readable sidecar alone does not solve live identity access. See
[Apple's process security policy](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/proc_info.c).
Do not weaken the identity match or use deprecated `kern.proc` SPI to hide
this support limit.

Tom ruled NARROW on 2026-10-06 in [Q-002](../questions.md#q-002--macos-gateway-cross-user-identity-access):
task 104 implements a bounded read-only identity command through Effigy's
existing administrator elevation. It may prompt for authentication. A
declined or non-interactive prompt yields unknown; lifecycle commands then
refuse to signal. The command reads only the trusted recorded gateway target,
returns bounded identity/status data, and does not signal, change records or
start anything.
No new helper, install, standing privilege, live-operations authority or change
to contract 010 is approved. The larger privilege model is not approved.
This authorizes the bounded implementation, not a live gateway migration.
Task 104 is in main and implements the sidecar plus fail-closed legacy policy.
Q-001's publication block remains until exact-candidate release assurance is
complete. The final identity-check-to-signal TOCTOU and the unrun live
cross-UID daemon proof remain documented limitations; this is not acceptance
of those risks or a release-readiness claim.

### Root-owned gateway restart follow-through — 2026-10-09

Tom requested a durable correction after repeated operator-account restarts
could not verify a healthy root-owned gateway and manual SIGKILL was used.
The bounded work first qualifies the existing identity/elevation path,
corrects demonstrated shutdown and terminal-handoff defects, and makes
refusal or authentication requirements visible within a bounded wait.
Private fixtures must preserve unrelated gateway, route and process state.

Successful HTTP traffic, a readable sidecar or a numeric PID alone still
cannot authenticate the daemon. Non-interactive reuse through a new
authenticated mechanism requires a concrete trust and compatibility proposal
and its own ruling before implementation; this request does not silently
widen Q-002, install a helper or grant standing privilege. The supported
interactive administrator path remains available. No live gateway migration,
manual signal, global binary replacement or release is authorized here.

The operator's installed `effigy v0.14.1+local.7b26a15` corresponds to source
commit `7b26a15`. At that source, `run_gateway` documents TERM/INT shutdown but
waits only for Ctrl-C. At task start, checkout `b0858d5c` already included the
generation sidecar, the bounded read-only identity reader and legacy recovery.
All three capabilities are also present and wired in source commit `7b26a15`;
the source comparison isolates the TERM-handler mismatch from those identity
and recovery features. The reported root daemon's historical reason for
ignoring TERM was not captured, so the missing TERM handler is a demonstrated
source defect, not a proven explanation of that earlier incident.

The corrected daemon registers Unix TERM and INT handling before publishing
its generation, sends the existing shutdown signal to its server tasks, and
joins all owned DNS, HTTP and HTTPS server tasks plus the TCP alias manager
before removing the exact unchanged identity record. Private-process proofs
cover ready listeners,
normal TERM/INT, inherited ignored TERM, owned PID cleanup, untouched routes
and a foreign listener. The last identity-check-to-signal interval remains a
TOCTOU.

Operator recovery stays on Q-002's existing authority. The read-only identity
reader remains bounded to 15 seconds and asks for visible administrator
approval only from an interactive terminal. Full elevated lifecycle waits are
bounded to 30 seconds; managed auto-start is bounded to 35 seconds and prints
the required operator action before waiting. A timeout reaps only the owned
launcher and reports that the gateway outcome is unknown; the caller does not
guess that an elevated child finished or remove records. Run `effigy gateway
status` before retrying. A non-interactive unknown identity remains held.

For a task-local development restart, use the managed session controls in
[guide 012](../../guides/012-dev-process-manager-tui.md#restart-the-managed-dev-session):
`effigy dev status`, `effigy dev stop`, then the same `effigy dev --headless`
task invocation. That restarts the managed dev session while the shared
gateway remains available to other sessions.

If Q-002's interactive reader cannot support prompt-free verification, see the
separate [read-only daemon attestation proposal](034-gateway-legacy-upgrade-recovery.md#proposed-read-only-root-daemon-attestation).
It is design evidence only; implementation needs a new operator ruling.

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

Disposable consumers can select a caller-owned private gateway with
`effigy gateway setup-tls --private-state-root <DIR>` and
`effigy gateway up --private-state-root <DIR>`. The existing root must be a
canonical owner-only directory; private mode keeps routes, process records,
listeners, certificates, and the mkcert CA under it. It binds DNS, HTTP, and
HTTPS only to loopback, with unprivileged ports (port `0` requests actual OS
allocation). Startup and status report the sockets held by that generation.
Private state validation currently requires Unix ownership and permission
checks; other platforms fail closed.
`EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=<DIR>` selects the same root for normal
container route registration and managed startup. Private mode does not install
CA trust, modify resolver files, provision host loopback aliases, or request
elevation. An absolute `EFFIGY_GATEWAY_MKCERT_BIN` selects the mkcert executable
for private certificate generation; otherwise Effigy searches its bounded
trusted install prefixes. Invalid explicit paths fail closed. Effigy passes
`<DIR>/ca` as `CAROOT` and invokes only certificate generation, never trust
installation. Clients explicitly trust `<DIR>/ca/rootCA.pem` and resolve each
route name to the reported HTTPS address; normal operator mode and its
established host integration remain the default. Fixed-port consumer adapters
and hybrid child-listener ownership are outside this capability.

A checkout's loopback alias is not a separate record: the gateway's route table
is the only record of loopback ownership, so an address is in use exactly when
some route references it as its `dns_ip` or as its target. `up` takes the
address the checkout's routes already answer with, or the lowest address in the
`127.1.0.1`-`127.1.0.50` pool that no route references; `down` and retirement
free it by removing those routes, and any route write drops the routes of a
checkout path that is missing. A deleted worktree therefore keeps neither a
domain nor an address, and no inventory across runtimes, qualified project key
or reclamation plan exists to run.

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
