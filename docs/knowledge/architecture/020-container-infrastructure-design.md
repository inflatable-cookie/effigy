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
`host:port` target and cannot be combined with `service` in one route. Linked
worktrees that do not share runtime identity rewrite the declared apex to
`<apex>-w<host-key>.<tld>` so HTTP routes and TCP aliases stay one set.
`effigy container hosts` exposes that map. The primary checkout keeps declared
names. Consumer apps still feed those names into public URLs, origins, cookies
and selectors; see Queue lead `af3ab7c7-b631-4d4f-9da7-f2949ded82f3`
for remaining consumer wiring.

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
| `effigy_host_run::canonical_start_identity` | Linux `{pid}@{boot}:{ticks}`; macOS `{pid}@UTC-whole-seconds` from `pbi_start_tvsec` only | Host-run contract 010 wire format truncates macOS start time to whole seconds. Do not change 010. Do not persist that string as the gateway record | Host-run peer proof only |
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
a legacy upgrade; corrected main provides `effigy gateway recover` on the
task-112/113 base.

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
still installed and does not authorize PID-only signaling. `effigy gateway
recover` is the implemented consumer path; see
[architecture 034](034-gateway-legacy-upgrade-recovery.md). `--yes` is only
the absent-record start; a live daemon requires `--adopt-candidate` and
interactive digest consent. A v0.13.1 rollback ignores `gateway.identity`
and restores the PID-only risk; downgrade is not claimed safe. Host-run
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
  signal, cleanup, or replacement. Startup text is state-keyed (`stopped`,
  `replacing a different build`, or `unverified`); an unknown or mismatched
  live daemon is never described as "down".
- The macOS boot identity was the raw `kern.boottime` timeval, whose
  microsecond field is not stable across reads. A sidecar written by the
  published v0.14.0 macOS build could therefore compare `Mismatch` against the
  live process it had itself published. `effigy_process::boot_identity` now
  reads the kernel `kern.bootsessionuuid` session identity through `sysctl`,
  requires a successful command status, and accepts only a canonical
  8-4-4-4-12 UUID; missing, failed, empty, non-UTF-8, or malformed output is
  Unknown. Linux is unchanged (`/proc/sys/kernel/random/boot_id`).

`effigy_process::boot_identity_matches` is the single comparison point. New
records store the session UUID and match exactly. Records written before the
change store the `kern.boottime` timeval; they match when the recorded `sec`
component equals a successfully read current `kern.boottime` seconds, which is
stable within a boot and distinct across a reboot at one-second resolution.
The exact process start identity stays mandatory for every match, so a reused
PID never matches. Unknown current evidence never matches, and no old sidecar
is reclassified as absent or rewritten. `probe_live_identity` uses this
comparison for both the ordinary and elevated readers. The legacy candidate
digest path is unchanged: it inspects and re-inspects within one CLI
generation, so it has no cross-version record to reconcile.

Shared boot-ID callers are unaffected on macOS: `effigy_host_run` derives its
wire identity from `pbi_start_tvsec` only, and the QA-group owner check uses
`process_start_identity`, not `boot_identity`. The QA-group record's
informational `boot_identity` field now carries the session UUID on macOS.

Private controls added by task 117: `effigy-process`
`boot_session_uuid_parser_rejects_missing_failed_and_malformed`,
`boot_session_identity_reader_is_stable_and_ignores_boot_time`,
`legacy_boot_time_identity_matches_across_microsecond_drift`, and the
separate-process `boot_identity_uncached_is_stable_across_separate_processes`;
`effigy-gateway`
`gateway_identity_legacy_boot_time_record_matches_across_microsecond_drift`
and the full-path
`gateway_identity_legacy_boot_time_sidecar_stays_running_across_drift`;
`effigy` `gateway_up_for_managed_task_preserves_terminal_stdin_and_diagnostics`
(real private PTY plus null-stdin controls) and
`gateway_up_for_managed_task_startup_notice_is_state_accurate`. Maintained
selectors: `test:gateway:boot-identity`, `check:gateway:boot-identity`,
`test:gateway:managed-tty`.

Residual: the last identity comparison and TERM/KILL syscall are separate
operations. A process can exit and its PID can be reused in that interval; this
bounded portable fix does not claim atomic process targeting or add pidfd.

### Gateway process identity ruling

Tom ruled on 2026-10-06: "Approved, but you need to fix this so other users
don't have the same problem." This approves the proposed one-time transition
of his existing legacy gateway: use a verified previous binary to stop the
confirmed daemon, confirm it is gone before reconciling only its PID/version
records, then start the current gateway. Routes, certificates, containers and
volumes are preserved. This is explicit authority for that host transition,
not a general exception allowing numeric-only records to authorize signals.
The consumer upgrade path must identify this legacy transition and provide
a supported recovery flow even when installation replaced the old executable.
Identity checks and fail-closed behavior remain required. The proposed design
covering the consumer transition, including the public channel posture and a
fix-forward patch runway, is
[034](034-gateway-legacy-upgrade-recovery.md). It keeps a numeric-only signal
unauthorized. Its separate
[operator ruling](034-gateway-legacy-upgrade-recovery.md#operator-ruling)
authorized bounded candidate inspection and generation-bound stop; it does not
extend this one-host ruling to previous-binary delegation.

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

An isolated project's loopback assignment uses the same qualified identity in
generated Compose and gateway registration:
`project:<project-name>:<absolute-checkout>`. A legacy bare project key moves
only when its saved scope matches that checkout and its IP is not assigned to
another identity. Scope retirement removes qualified and attributable legacy
keys; shared identities remain retained. Stale reclamation requires
successful inventory across every participating runtime: Docker when it
participates, and every running Colima profile. Participation is decided
first, without launching a runtime. Docker considers every endpoint that could
be effective (`DOCKER_CONTEXT`, `DOCKER_HOST`, the stored current context, then
the platform default socket) and is inactive only when all of them are absent
local Unix sockets; any remote, reachable, unresolvable, unreadable, or stale
endpoint stays authoritative. Context names are case-sensitive, so a context
named `DEFAULT` stays selectable. Colima is inactive only when its CLI is
absent and its Colima home can be inspected with no Colima or Lima instance
state remaining. A successful empty result is
distinct from profile-list, `ps`, or parse failure. Incomplete discovery
preserves uncertain assignments and reports the backend, profile, error, and
skipped reclamation reason; identical warnings print once per process. If
preservation leaves the bounded pool full, allocation reports capacity
exhaustion with the discovery failure.

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
