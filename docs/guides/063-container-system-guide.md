# 063 - Containers for Local Dev

Use this guide when a repo wants a host-clean local environment with:

- databases, queues, or other backing services
- a workspace container for Linux-native app work
- local domains and gateway routing
- explicit lifecycle control through `effigy container ...`

This is the practical command guide for the container commands.

Use:
- this guide for the direct `effigy container ...` commands and container
  manifest shape
- [`064-system-workspace-and-dev-contract.md`](064-system-workspace-and-dev-contract.md)
  for the model behind `system`, `workspace`, and `dev`
- [`067-catalog-services-reference.md`](067-catalog-services-reference.md)
  for the shipped service catalog inputs

## Start Here

Shortest path:

1. declare a named container in `effigy.toml`
2. run `effigy container up`
3. use `effigy container shell` or a task such as `effigy dev`
4. run `effigy container down` when you want the environment gone

## Host prerequisites (macOS)

Container features are host-dependent. Install only what the features you use
require.

### Containers (required)

Effigy supports two local backends today:

- Colima/containerd
- Docker Desktop

Repo manifests still declare their runtime intent, so many repos remain
Colima-first. Docker/Desktop is now a first-class override and bootstrap path,
not a side escape hatch.

```bash
brew install colima
```

Effigy starts the configured Colima **profile** on demand (from
`[containers.<name>].profile`). You can still manage it directly when needed:

```bash
colima list
colima stop --profile <profile>
```

Docker Desktop is optional. Install it when a repo or bootstrap session should
run through the Docker backend:

```bash
brew install --cask docker
```

Useful backend controls:

```sh
effigy config set containers.backend containerd
effigy config set containers.backend docker
effigy bootstrap <git-url> --backend docker
effigy doctor --verbose
```

### OCI artifacts (optional)

If you use `data dump --push`, `data seed` with OCI refs, or `effigy artifact`
commands, install `oras`:

```bash
brew install oras
oras login ghcr.io
```

Effigy uses the normal registry-client auth store. It does not accept tokens in
artifact refs or env files.

### HTTPS gateway routes (optional)

If you use local domains with `tls = true`, install `mkcert` and run the
one-time trust-store install:

```bash
brew install mkcert
mkcert -install
```

Effigy also provides `effigy gateway setup-tls` as the “do the right thing”
helper for mkcert-backed TLS.

Common commands:

```sh
effigy container up
effigy container <NAME> up
effigy container down
effigy container <NAME> down
effigy container status --global
effigy container stats --global
effigy container <NAME> status
effigy container profile status
effigy container profile resize
effigy container profile purge --yes
effigy container profile recreate --disk 180 --yes
effigy container <NAME> logs
effigy container <NAME> shell
effigy container <NAME> data list
effigy container <NAME> data export <VOLUME> <PATH>
effigy container <NAME> data import <VOLUME> <PATH>
effigy container <NAME> data pull-production
effigy container <NAME> reset --keep-data
effigy container <NAME> eject
effigy container hosts
effigy container scope [--repo <PATH>] [--json]
effigy container retire --yes
effigy container cache list --global
effigy container volume list --dormant
```

Default resolution:

- when `<NAME>` is omitted, Effigy resolves `[containers].default`
- if no default exists, Effigy fails clearly instead of guessing

Useful flags:

- `--attach` / `--detach` override manifest startup mode for `up`
- `--service <NAME>` focuses `logs` or `shell` on one service
- `--global` turns `status` and `stats` into cross-project views
- `--command <CMD>` runs one command string inside the service via `sh -lc`
- `reset` preserves persistent named volumes by default
- `--wipe-data` deletes persistent named volumes during `reset`
- `--keep-data` remains accepted as a compatibility alias for reset's default
- `--json` returns machine-readable payloads for non-interactive paths
- `hosts` prints the effective HTTP and TCP host map for this checkout
- `retire --yes` removes only this runtime scope's owned resources; `--scope <TOKEN>` retries after the checkout directory is gone

## QA Recipe

When container shell behavior changes, rerun the two repo-owned checks below
before trusting the result.

### 1. Prove exec readiness handling

This task pins the drift path end to end:

- status reporting exposes `primary_service_exec_ready`
- runtime status warning text stays stable
- runner recovery for a non-exec-ready primary service still works

```sh
effigy qa:architecture:container-exec-readiness
```

Use this when the change touches:

- `container status`
- workspace or container handoff
- primary-service exec probes
- runtime drift or recovery behavior

### 2. Re-profile the live shell path

This task runs the real `container shell --command 'true'` path against the
maintained local fixture matrix and writes reports under:

- `.effigy/perf/container-shell-matrix/README.md`
- `.effigy/perf/container-shell-matrix/summary.json`
- `.effigy/perf/container-shell-matrix/*.md`

```sh
effigy perf:container-shell-matrix
```

Each report now includes:

- full `container status` output
- `primary_service_exec_ready`
- traced backend invocations
- steady-state timings

The JSON summary is the compact machine-readable view:

- one entry per target repo
- `primary_service_exec_ready` as a boolean
- traced `real` time as the exact `/usr/bin/time -p` value
- steady-state `real` timings as exact `/usr/bin/time -p` values
- the matching markdown report path

Current matrix intent:

- a decodelabs library fixture
- a decodelabs app fixture
- an underlay workspace fixture

If a timing result looks good but `primary_service_exec_ready` is `no`, treat
that runtime as unhealthy and fix the drift before trusting the number.

If you need to compare runs later, keep the emitted `summary.json` as a saved
baseline and diff it outside the task. The repo-owned surface stops at
producing one clean cross-repo report plus one compact machine-readable
artifact.

## When To Use `container`

Use `container` when you want to:

- bring a declared local environment up or down directly
- inspect logs or open a shell in one container service
- export, import, reset, or eject environment data
- operate a simple compose-backed repo without the fuller `system` setup

If the question is instead:

- how should `dev`, `system`, and `workspace` fit together?
  - use [`064`](064-system-workspace-and-dev-contract.md)
- which service inputs does a catalog support?
  - use [`067`](067-catalog-services-reference.md)

## Manifest Shapes

Two shapes are supported.

- prefer catalog-driven generated compose for normal use
- use a repo-owned `compose_file` only when the repo genuinely needs direct
  compose ownership

### Catalog-Driven

```toml
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "effigy"
project_name = "my-app-dev"
primary_service = "app"

[containers.web.services.app]
catalog = "workspace-rust-bun"

[containers.web.services.db]
catalog = "postgres"

[containers.web.services.cache]
catalog = "redis"

[containers.web.dns]
domain = "my-app.test"
tls = true

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 10

[containers.web.host]
mounts = ["./:/workspace"]
```

`workspace-rust-bun` stays a toolchain image by default (`browser_runtime =
"none"`). Set `browser_runtime = "chromium"` on that service to install
Debian Chromium shared libraries and a basic font set at image build. The
image does not ship Chromium, Playwright, or Node; the consuming repo still
downloads its own browser revision. Changing the value requires an image
rebuild. See [`067`](067-catalog-services-reference.md) for the full
parameter surface.

Effigy generates runtime-owned compose output under:

- `.effigy/runtime/compose/.effigy-compose.generated.yml`
- `.effigy/runtime/compose/.effigy-catalog/<service>/...`

Treat that directory as runtime output, not repo-owned source.

Runtime-scoped checkouts — linked Git worktrees and full clones whose local
Git config sets `effigy.runtimeScope = ephemeral` — get a stable generation
suffix on generated Compose project names (`-wt-` for a worktree, `-ec-` for
a marked clone). Containers, networks and managed volumes therefore use
different identities from the primary checkout, other worktrees and other
clones. Generated host ports are allocated separately, even if `host.ports`
declares fixed numbers. The primary checkout and any unmarked clone keep
their existing name and ports. Repeated commands in one scoped checkout
reach the same stack; deleting and recreating it at the same path gets a
new identity. To intentionally share one Compose stack, set
`share_runtime_identity = true` under `[containers.<name>]`.

The clone marker is read from that checkout's own `.git/config` only — never
from global or system Git configuration, and `[include]` directives are not
followed. Only the tool that created the clone (Nucleus sets this marker on
every task workspace clone) can scope it. A marked clone owns its `.git`
directory outright, so no linked-worktree shared-Git-directory mount is
added to its container.

Scoped checkouts also get a distinct gateway host map. Declared `app.test`
becomes `app-w<host-key>.test`; helpers such as `mail.app.test` stay under
that apex. The primary checkout and unmarked clones keep the declared
names. `effigy container hosts --json` is the machine-readable map for
public URLs, origins, cookie scope, WebAuthn and test selectors, and
reports scope kind `worktree` or `ephemeral-clone`. Effigy does not
rewrite application configuration. `effigy container retire --yes` removes
that scope's owned containers (running, stopped, or Created), networks,
disposable caches, mutable volumes, isolated routes and ports using
`com.effigy.scope`, `com.effigy.project`, or the exact Compose project
label, and only on the runtime profile that produced each observation.
Shared services, persistent volumes, external volumes, and shared-identity
routes stay. Repo-owned Compose volumes stay unless labelled
`com.effigy.persist=false`. A record under `~/.effigy/runtime-scopes/`
survives a deleted checkout and aggregates every environment and runtime
profile in that scope so the same command can retry. Record writes are
locked and atomic; corrupt JSON fails closed. A backend that cannot list
resources, or that returns malformed inspect output, fails instead of
reporting an empty stack. TLS certificates for isolated routes are removed
under the route lock after an owner check; failure leaves the route and a
pending-cert list on the record. A pending certificate whose domain now has
a foreign owner stays.

Archive callers that need the identity before the checkout disappears should
run `effigy container scope --json`. It reports the absolute checkout path,
scope kind and full generation token without loading a container policy or
contacting a backend. A scoped checkout with no token yet creates it in the
private Git directory; a primary checkout or unmarked clone returns kind
`none` and a null token. Persist the token before archive, then pass it to
`effigy container retire --scope <token> --yes` if cleanup must be retried
after the checkout is gone.

Paseo worktree archive should call retire from `paseo.json`
`worktree.teardown` before the checkout disappears. After the directory is
gone, retry with `effigy container retire --scope <token> --yes`. Do not
use `bootstrap teardown` for worker scopes; that command is only for
`--fresh` bootstrap sessions. A failed retire must stay non-zero so the
durable record remains and archive does not swallow residue.

Generated compose binds every published port to loopback by default —
the port policy rewrites fragment entries like `"3000:3000"` into
`"127.0.0.1:<allocated-host-port>:3000"`, so dev services (dbgate,
postgres, minio, mailpit, ...) are reachable from the local machine and
the gateway's TCP aliases but not from the LAN. Opt back into public
binding per container environment:

```toml
[containers.web.host]
publish_address = "0.0.0.0"
```

### Repo-Owned `compose_file`

```toml
[containers.web]
driver = "colima"
startup = "attached"
profile = "effigy"
compose_file = "infra/dev/docker-compose.yml"
project_name = "my-app-dev"
primary_service = "app"

[containers.web.host]
ports = ["8080:80", "3306:3306"]
mounts = ["./:/workspace"]
```

Use this when:

- the repo already owns its own compose file
- the repo has taken local ownership through `effigy container <name> eject`
- the generated catalog path is not sufficient

Runtime-scoped checkouts using a repo-owned Compose file still get a scoped
project name. Effigy refuses fixed published ports, `container_name`, and
named or external top-level volumes and networks in that file because it
cannot safely rewrite repo-owned Compose resources. Remove those fixed
resources, use generated Compose, or explicitly opt into a shared runtime
with `share_runtime_identity = true`.

## Core Rules

Current v1 rules:

- repo-bound operations follow the manifest driver unless a stronger
  session-scoped override is active
- Docker/Desktop is a supported backend for bootstrap and runtime use, not just
  an ambient CLI fallback
- `primary_service` is required
- `compose_file`, when used, must stay repo-relative
- repo-relative `host.mounts` may not escape the repo root unless they use the
  structured `external = true` form
- generated-compose data lifecycle stays on the product-owned path instead of
  widening direct `compose_file` ownership

## External Host Mounts

`[containers.<name>.host].mounts` accepts:

- legacy `"source:target[:options]"` strings for repo-relative sources
- structured tables for out-of-repo mounts

```toml
[containers.web.host]
mounts = [
  ".:/workspace",
  { host = "${PERSONAL_SSH_CONFIG}",
    container = "/home/dev/.ssh/config",
    external = true,
    options = ["ro"] },
]
```

Rules:

- `host` supports `${VAR}` and `~`
- without `external = true`, absolute resolved paths are rejected
- with `external = true`, the path may be absolute or repo-relative
- the source must exist when policy loads
- `options` is passed through to the compose layer unchanged

Use this with `effigy.local.toml` for per-machine mounts.

## Runtime Behavior

`effigy container up` means:

1. ensure the selected runtime is usable; for Colima containerd, Effigy waits
   for profile-scoped `nerdctl info` and BuildKit workers, and can repair a
   running but incomplete profile once
2. bring the compose environment up
3. inspect owned Compose projects including stopped containers and `start`
   currently declared services that are still `Exited` or `Created`; this
   skips one-off `compose run` containers and undeclared orphans, keeps
   volumes, including Postgres crash-recovery data, and does not delete
   systemd units
4. wait for declared environment readiness when present
5. wait for each declared gateway service port to appear on the matching
   Compose project before registering its route
6. either return immediately or attach, based on startup mode

These waits are bounded. A timeout names the runtime or gateway readiness
stage; a stopped Compose service is reported with its exit status. After a
Colima VM restart, a stale nerdctl health-check timer warning is recorded
when an owned container actually starts; if it stays Exited, `container up`
fails with that backend text and
`colima nerdctl --profile <profile> -- start <container>`.
`container status` lists stopped owned services instead of omitting them.
Gateway registration keeps the project, service, published-port and host-listener
ownership checks, and does not claim a route while its declared runtime
binding is missing.

Attached mode behavior:

- attached mode is the default when `startup = "attached"`
- Effigy owns that session lifecycle
- Ctrl+C follows the normal shutdown path
- `on_task_exit = "stop"` is the default shutdown posture
- interactive runs use Effigy's tabbed session runtime
- non-interactive runs fall back to stream-mode output

`effigy container down` is intentionally narrow:

- it tears down the compose environment
- it does not stop the Colima profile itself

Cross-project views:

- `effigy container status --global`
- `effigy container stats --global`
- `effigy container cache list --global`
- `effigy container volume list --global`

Repo-local cleanup views:

- `effigy container cache list`
- `effigy container volume list`
- `effigy container volume list --dormant`

Use `cache` for disposable build/install artifacts. Use `volume` for named
volume ownership, dormant repo leftovers, and global orphan discovery.

### Runtime inventory participation

Loopback stale reclamation and the cross-project `--global` views read the
running Compose inventory. Before any `ps`, Effigy decides whether each
runtime is participating, without launching Docker, Colima, or a container:

- Docker participates when any endpoint it could be using — `DOCKER_CONTEXT`,
  `DOCKER_HOST`, the stored current context, or the platform default socket —
  is remote or reachable. Docker is inactive only when every candidate is an
  absent local Unix socket, so Docker Desktop not running means Docker is
  skipped rather than reported as a failed inventory. A stale, unreadable,
  schema-invalid, or unresolvable endpoint stays fail-closed, and a remote
  context selected by `DOCKER_CONTEXT` is never omitted by an absent local
  `DOCKER_HOST`. The `DOCKER_CONFIG` path and context names are read exactly
  as given.
- Colima participates when its CLI is installed, and also when its CLI is
  missing but Colima or Lima instance state remains, because a running Colima
  VM can outlive its client CLI. Only a host with neither is Colima-inactive.

A proven-inactive runtime cannot poison an otherwise complete inventory, so a
Colima-only host reclaims stale isolated loopback assignments without starting
Docker Desktop. Genuine failures still make the inventory incomplete, preserve
every uncertain assignment, and report the backend, profile, and error once
per launch.

## Task Activation

Container-backed tasks now use two distinct lifecycle models.

### Public Shell Sessions

Interactive public shell/session surfaces include:

- `effigy dev`
- `effigy workspace`
- tasks that end in `stay_in_shell = true`

These are session-owned lifecycles:

- Effigy prepares the runtime for an interactive shell or handoff
- public gateway routes are reconciled before the shell opens
- the session asks whether to bring the container down on exit and defaults to
  yes
- `on_task_exit` still governs attached container-session shutdown, but public
  shell exits now use the explicit exit prompt instead of ownership heuristics

This is the right model when the user is being dropped into a live workspace.

### Non-Shell Container Tasks

Non-interactive container-backed tasks include:

- explicit tasks with `run_in = "container"`
- deferred requests with `[defer].run_in = "container"`
- bootstrap `run` steps that route into a container but do not open a shell
- builtin `effigy test` suites of a catalog whose declared runtime target
  resolves a named container (each target resolves with the same binding
  grammar tasks use; host-scoped catalogs keep suites on the host)

These use shared task activation instead of session ownership:

- Effigy auto-starts the runtime when needed
- sibling-service bring-up and exec-readiness recovery run before dispatch
- a runtime-scoped checkout (linked worktree or marked ephemeral clone)
  proves its selected checkout is readable and writable in the primary
  service before dispatch; a mismatched mount stops the task before lease
  refresh. Changed Cargo inputs are touched inside the container so host edits
  invalidate stale build artifacts.
- public gateway/runtime route registration is reconciled when the container
  declares a gateway surface
- if the task had to start the runtime, or the runtime was already under an
  active task lease, Effigy refreshes a temporary host-container lease

Default lease behavior:

- lease duration defaults to 5 minutes
- the lease is refreshed on reuse
- the runtime shuts down after the lease expires unless the user explicitly
  keeps it up through `effigy container up`, `effigy dev`, or another owned
  session

The goal is one warm-runtime contract for non-shell tasks instead of separate
behavior for deferred versus explicit container routing.

For a fresh Bun worktree, prepare dependencies with `effigy bootstrap deps
sync <path>` before QA. The default path uses the committed lock with `bun
install --frozen-lockfile`. Built-in tests report this route when a selected
Bun suite has no installed dependencies, before reporting source test failures.

### Workspace Exec Identity

Host-routed tasks and `effigy exec` use the container's declared
`workspace_user` and `workspace_home` when they target the primary service.
Explicit execs against another service keep that service's configured user.
Interactive terminals retain a TTY; pipes, agents, and other non-console
callers run without one.

For a running workspace, `effigy doctor` scans primary-service managed volumes
and `$BUN_INSTALL/install` for root-owned paths. A regression is reported as
`container.workspace-ownership`, with one sample from each affected mount.
The check is read-only and skips stopped workspace services.

## Data Lifecycle

Generated-compose environments support:

- `data list`
- `data export`
- `data import`
- `data pull-production`
- `data dump`
- `data seed`
- data-safe `reset`
- explicit `reset --wipe-data`
- `eject`

These stay on the generated-compose path so Effigy can keep the runtime data
behavior predictable.

`data import`, `data pull-production`, and `reset --wipe-data` are guarded
because they can overwrite or delete local generated-compose data. In a real
interactive terminal, Effigy asks for confirmation and defaults to no. In
non-interactive or JSON mode it does not prompt; automation must pass `--yes`
when the data change is intentional.

### Data Dump and Push

`effigy container data dump` exports logical SQL dumps from generated-compose
database services. By default it writes local files:

```sh
# Bare target writes ./<target>.sql
effigy container data dump legacy_mysql

# Explicit local path
effigy container data dump app=./app.sql app_test=./app_test.sql
```

You can also target an OCI registry. This stages the dump locally and reports
the planned destination without publishing:

```sh
effigy container data dump app=oci://ghcr.io/acme/uat-content:2026-05-07 --json
```

To publish after staging, add `--push`:

```sh
effigy container data dump app=oci://ghcr.io/acme/uat-content:2026-05-07 --push --json
```

Push rules:

- `--push` is required for live registry writes
- local-only dumps reject `--push`
- digest-pinned refs are invalid push destinations
- the pushed digest is reported in JSON output
- use `oras login` beforehand for registry auth

### Data Seed

`effigy container data seed` stages local or OCI artifacts and invokes the
standard `bootstrap:db-seed` task. It currently targets the repo default
container only.

```sh
# Local file
effigy container data seed --db-seed ./latest.sql

# OCI artifact
effigy container data seed --db-seed app=oci://ghcr.io/acme/private-data:uat

# Multiple targets
effigy container data seed --db-seed cbs=./cbs.sql --db-seed cbs-mortcalc=./mortcalc.sql
```

OCI refs must use the explicit `oci://` prefix. `data seed` stages the artifact
under `.effigy/local/db-seeds/` before the app-specific seed logic runs.

## Cache Lifecycle

`effigy container cache list` and `cache prune` manage purge-safe isolated
build cache volumes. These are disposable build artifacts, not persistent app
data.

Cache kinds Effigy recognizes:

- `rust-target` — Rust `target` directories
- `node-modules` — package manager `node_modules` directories
- `pnpm-store` — pnpm content-addressable store volumes outside the repo bind mount

Cache volumes are created by catalog-generated compose files based on mount
target heuristics. Legacy generated compose volumes with opaque `efv-*` names
are also classified by their mounted contents, so old Rust target volumes show
up as `rust-target` even when the volume name does not contain `target`.
`cache list` inventories them and reports reclaimable and in-use size
totals in text and JSON. Missing per-volume sizes keep `size_complete`
false instead of pretending the total is complete. `cache prune` removes
the same inventory and already reports removed/skipped size totals.

Cache volume names include the container workspace path, so moving a mount from
`/var/www/html` to `/var/www/inventors` gives a fresh `node_modules` volume
instead of reusing one with stale absolute-path metadata.

```sh
# List caches for the current repo
effigy container cache list

# List caches across all projects on the Colima profile
effigy container cache list --global

# Filter by project or kind
effigy container cache list --project example-app-dev
effigy container cache list --kind rust-target

# Prune (requires confirmation)
effigy container cache prune --yes
effigy container cache prune --kind node-modules --yes
effigy container cache prune --global --yes
effigy container cache prune --kind rust-target --yes
```

Safety rules:

- running projects are skipped in `--global` mode
- prune requires confirmation unless `--yes` is passed
- volume orphan cleanup is separate; use cache cleanup for Rust `target`
  pressure, not `container volume prune --global --orphans`
- cache volumes are recreated automatically on the next build

## Volume Lifecycle

`effigy container volume list` and `volume prune` manage Effigy-managed named
volumes, including persistent app data and orphaned volumes.

`volume list` is read-only. By default it shows volumes for the current repo:

```sh
# List volumes for the current repo
effigy container volume list

# Show repo-scoped volumes that are no longer declared (superseded)
effigy container volume list --dormant
```

Add `--global` for a machine-wide view across all runtimes:

```sh
# List all Effigy-managed volumes across repos
effigy container volume list --global

# Show only orphaned volumes whose owning repo is gone or no longer declares them
effigy container volume list --global --orphans
```

`volume prune` removes volumes. It requires either `--dormant` for repo-scoped
cleanup or `--global --orphans` for machine-wide orphan cleanup:

```sh
# Remove superseded volumes for the current repo
effigy container volume prune --dormant --yes

# Remove orphaned volumes across all repos
effigy container volume prune --global --orphans --yes
```

Safety rules:

- `--orphans` is only valid with `--global`
- `--dormant` and `--global` are mutually exclusive
- prune requires confirmation unless `--yes` is passed
- persistent app data volumes are never included in orphan or dormant cleanup

## DNS and Gateway

`[containers.<name>.dns]` integrates with the gateway:

- route registration and cleanup happen during container lifecycle
- `tls = true` works with `effigy gateway setup-tls`
- when TLS is enabled, plain HTTP requests are redirected to HTTPS

TCP catalog services such as postgres, mariadb, redis, and memcached also get
deterministic loopback aliases. That means host and container code can use the
same stable names without hand-written `/etc/hosts` edits.

Scoped checkouts — linked worktrees and marked ephemeral clones — register
the effective host map from `effigy container hosts`, not the raw declared
domains. If another live checkout already owns a name, registration fails
and names the owning checkout. This also protects TLS certificates and TCP
service aliases: stopping one scope never removes a sibling's current
route. A route from a retired checkout generation can be reclaimed on the
next registration. `effigy gateway repair --yes` can clean stale TCP alias
conflicts, but leaves live checkout routes alone.

### Route-table trust

The gateway daemon can run elevated (to bind `:80`/`:443` and write resolver
files), and it reverse-proxies to whatever upstream each route names. So it
verifies its route table before trusting it: Effigy writes `routes.json`
owner-only (`0o600`) with a managed marker, and the daemon refuses a
group/other-writable or unmarked table, keeping its last-known-good routes
instead. `effigy gateway status` reports `route_table_trust` and
`effigy doctor` warns when the table is untrusted. If you upgrade from a build
that predates this and see an "untrusted" warning, re-run `effigy container up`
(or re-register routes) once to re-stamp the table. The full model is in
[`033-gateway-route-table-trust-contract.md`](../knowledge/contracts/033-gateway-route-table-trust-contract.md).

## Troubleshooting: stale SSH-agent forwarding

Symptom: `effigy container up` fails the workspace container with

```
fatal: failed to mkdir "/run/host-services/ssh-auth.sock": ... file exists
```

Cause: Colima's `--ssh-agent` forwarding points a symlink at
`/run/host-services/ssh-auth.sock` inside the VM to the host's SSH-agent
socket. On a long-running VM the host agent socket can rotate, leaving that
symlink **dangling**. The container runtime cannot bind-mount a dangling-symlink
source, so the workspace container never starts. (Not an Effigy regression — the
compose is unchanged.)

Fix: restart the profile so Colima re-forwards a live socket, then bring the
workspace back up:

```bash
colima restart <profile>
effigy container up
```

Effigy pre-empts this: `container up` warns when the forwarded socket is stale
before attempting the mount, and `effigy doctor` flags it for any running colima
profile — both naming the `colima restart <profile>` remediation.

## Troubleshooting: services stay Exited after Colima restart

Symptom: after the Colima VM restarts, Postgres or MySQL may stay `Exited`
while `nerdctl compose up` returns success. Manual `nerdctl start` works and
may log `Unit <id>.timer was already loaded or has a fragment file`.

Cause: `nerdctl compose up` can succeed without starting persisted
containers. nerdctl health-check systemd timers are named after container
ids; leftover timer units produce that warning. The warning is a nerdctl
defect, not proof that Effigy owns those units.

Fix: re-run `effigy container up`. Effigy starts only currently declared
Exited/Created services (Compose project label match), skips one-off
`compose run` containers and undeclared orphans, keeps volumes, and does not
delete systemd units. Inspect and start are bounded; a hung nerdctl
command still names the last observed status. Start exit 0 is not
readiness: if the service stays stopped, the error keeps the start
backend text (including a stale-timer warning) and names:

```bash
colima nerdctl --profile <profile> -- start <container>
```

`effigy container status` shows the live `Exited` row rather than an empty
or assumed-running service list.

## Host Runtime Fallback

On hosts with Docker CLI installed, Effigy uses `docker compose`.

When Colima exists but `docker` is not on `PATH`, Effigy falls back to:

- `colima nerdctl -- compose`

with the profile started under `containerd`.

Effigy manages the default `effigy` Colima profile for workspace-heavy local
development. New or recreated profiles use the configured
`containers.profile_disk_gib` user-global target, falling back to 300GiB, plus
memory and swap sizing based on host memory. Existing smaller profiles may need
a manual resize or recreate; Effigy warns when a running managed profile is
below the target and points at the non-destructive resize path first.

Use the profile commands for that workflow:

```sh
# Inspect actual profile sizing against Effigy's managed targets
effigy container profile status

# Apply the managed sizing in place by stopping and restarting the profile
effigy container profile resize

# Persist a different managed disk target
effigy config set containers.profile_disk_gib 180

# Delete the managed profile and all profile runtime data without restarting it
effigy container profile purge --yes

# Recreate only if resize cannot get the profile to the managed target
# This deletes local profile data, including containers, images, and volumes.
effigy container profile recreate --disk 180 --yes
```

When `container profile recreate` runs interactively without `--disk`, Effigy
prompts for the disk size before continuing. Non-interactive runs use the
configured/default target unless `--disk` is supplied.

## Current Limits

Still intentionally narrow:

- no broad multi-driver abstraction
- no richer per-service health DSL yet
- direct `compose_file` ownership remains narrower than generated-compose

## Related

- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](055-everyday-workflows.md)
- [`064-system-workspace-and-dev-contract.md`](064-system-workspace-and-dev-contract.md)
- [`072-artifact-commands-guide.md`](072-artifact-commands-guide.md)
- [`014-artifact-substrate-contract.md`](../knowledge/contracts/014-artifact-substrate-contract.md)

## Next Step

Use this page when deciding whether a repo should stay on direct
`effigy container ...` commands or move to the broader `system` and
`workspace` model.
