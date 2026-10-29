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
  for the model behind `container`, `[systems]`, `workspace`, and `dev`
- [`067-catalog-services-reference.md`](067-catalog-services-reference.md)
  for the shipped service catalog inputs

## Inspect the exact environment render

Run `effigy container render <NAME>` (or add `--json`) to inspect the exact
Compose files and generated assets, generation path and digest without changing
runtime state. The reported path is prospective until `container up` prepares
and publishes that immutable generation. Unassigned ports stay explicit in the
render. Existing generated files migrate automatically on the next Prepare.
Reset removes only that environment's references; sibling renders are preserved.

Each catalog `isolated_dirs` path is a separate named volume over the source
bind, including root `node_modules`, nested dependencies and Rust targets in
worktree stacks. Mounts are part of a service's rendered configuration, so
there is no separate mount-drift detector: `container up` runs one apply
(`compose up -d --build --remove-orphans`, no recreate flag) and Compose
recreates exactly the services whose configuration changed, mounts included. A
live task holder keeps the running stack and is attached to it; it is never
recreated under.

An isolated volume overlays a directory inside the checkout, so that host
mount point must exist for the runtime to mount over it. `container up` and
routed-task preparation create a missing host directory for the primary
service's nested isolated volumes before the apply. The created directory never
replaces a file and never follows a symlink. Effigy never repairs a host bind
by recursively changing its ownership.


## Start Here

Shortest path:

1. declare a named container in `effigy.toml`
2. run `effigy container up`
3. use `effigy container shell` or a task such as `effigy dev`
4. run `effigy container down` when you want the environment gone

## Catalog paths in container tasks

Existing standard commands such as `run = "cd {repo} && bun x tsc --noEmit"`
keep their placeholders when a host aggregate invokes a container leaf.
`{repo}` and `{project}` become the selected catalog's container-mapped path;
a nested catalog keeps its suffix under the workspace mount. The same aliases
in task environment values use that path. Host tasks continue to use host
paths, and env-file/configuration lookup stays on the host.

Use `effigy <catalog>/<task> --plan` to inspect the selected runtime and rendered
command. Catalog and `cwd` metadata still describe host selection. Arbitrary
literal host paths and arguments are preserved, so a hard-coded host path is
not automatically portable. Mapping a command does not prove that its service
mounted the workspace. Mixed container sequences keep their existing handoff
behavior; this does not add inline standard-task support or Linux admission.

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
effigy doctor --verbose
```

### OCI artifacts (optional)

If you use `data dump --push`, `data seed` with OCI refs, or
`effigy state artifact` commands, install `oras`:

```bash
brew install oras
oras login ghcr.io
```

Effigy uses the normal registry-client auth store. It does not accept tokens in
artifact refs or env files.

### HTTPS gateway routes (optional)

In ordinary operator mode, if you use local domains with `tls = true`, install
`mkcert` and run the one-time trust-store install:

```bash
brew install mkcert
mkcert -install
```

Effigy also provides `effigy gateway setup-tls` as the “do the right thing”
helper for mkcert-backed TLS. For disposable mode without host trust changes,
use the [private gateway workflow](#private-gateway-for-disposable-consumers)
instead; do not run `mkcert -install` for that fixture.

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
effigy container migrate <NAME>
effigy container hosts
effigy container scope [--repo <PATH>] [--json]
effigy container retire --yes
effigy container retire --dead --yes
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
- `--bounded` opts a `--command` run into bounded finite capture and requires `--deadline-ms`, `--stdout-limit-bytes` and `--stderr-limit-bytes`; a bounds failure, read failure, incomplete output or deadline is an error even when the inner command exits `0`, and `--json` emits the `effigy.container.bounded-exec.v1` payload on both success and refusal
- `reset` preserves persistent named volumes by default
- `--wipe-data` deletes persistent named volumes during `reset`
- `--keep-data` remains accepted as a compatibility alias for reset's default
- `--json` returns machine-readable payloads for non-interactive paths
- `hosts` prints the effective HTTP and TCP host map for this checkout
- `retire --yes` removes every container, volume and network the backend labels for this checkout; `--keep-data` keeps the volumes and lists them, and `--dead` retires a stack whose checkout path is gone

### Bounded finite command capture

The plain command shell captures the whole command output before emission with
no caller deadline and no stream budget. When a caller needs a hard bound, opt
in explicitly:

```sh
effigy container web shell \
  --command "php artisan some:report" \
  --bounded --deadline-ms 30000 \
  --stdout-limit-bytes 1048576 --stderr-limit-bytes 262144 --json
```

Each bound is a strictly parsed positive integer validated before any effect
(`--deadline-ms` 1..3600000, stream limits 1..67108864). One absolute deadline
is created before runtime discovery and carried through runtime liveness and
identity inspection, primary workspace Effigy identity and architecture
probes, artifact build or download, service copies and install captures, user
command execution and the final drain. Each process capture uses the same
bounded reader; requests and processes use the remaining deadline. A phase that
returns after expiry refuses before the next phase or user command is spawned.
Synchronous local filesystem calls cannot be interrupted mid-call; the shared
deadline is checked before the next external operation and at phase boundaries.

Discovery also proves which container the command will reach: one compact,
budgeted runtime listing must show exactly one running, non-oneoff container of
the selected service whose checkout and scope labels name this checkout. Every
later effect (workspace artifact and catalog copies, and the command itself)
targets that container by full ID, so a same-named project in another checkout
is never used. The listing is one extra preparation capture of roughly a
hundred bytes.

Workspace identity refresh keeps the existing primary-service repair selection,
but the bounded runtime may refuse a repair that the plain shell would have
more time to complete. The repair uses the operation's remaining deadline and
budgets; bounded refusal does not fall back to unbounded shell preparation or
launch the requested command.

The stdout and stderr budgets are cumulative across the whole operation,
including preparation captures and the existing missing-workspace-user
fallback, so neither preparation nor a second attempt can reset the hard total.
`retained` reports all bytes retained by the operation. The returned `stdout`,
`stderr`, `stdout_bytes` and `stderr_bytes` contain only the completed user
command capture. JSON also reports `preparation.captures` and per-stream
preparation retained counts; these are additive fields in schema v1. Those
retained counts cover process stdout/stderr; downloaded artifact bytes are not
stream output. Local manifest and artifact selection checks are synchronous
and do not capture command output.

Overflow of either stream, a stream read failure, incomplete output, or an
expired deadline all refuse, and the retained prefix is never reported as
success. Selection stays on the existing checkout/profile/service/working-
directory resolution. A complete in-budget capture with a nonzero inner exit
still fails with the inner code, which is distinct from the bounds and deadline
reasons. Human mode emits retained command stdout/stderr only after a successful
inner exit; a nonzero exit returns safe failure details without writing captured
bytes.

The `--json` payload is versioned `effigy.container.bounded-exec.v1`. Success
returns the bounded command bytes and counts; a refusal returns a stable reason,
phase, optional stream name, limit and safe counts, plus a versioned
`refusal_observations` block. Every observation in that block is structural and
secret-free, taken at one snapshot from the original operation clock:
`operation_elapsed_ms`, `remaining_budget_ms`, `preparation_elapsed_ms` and
`attempt_elapsed_ms` (the attempt fields are null when no attempt began), a
`spawn_disposition` (`not_attempted`, `attempted_not_confirmed`, `spawned` or
`unknown`) for the owned host client only, and per-stream `stdout`/`stderr`
objects with a `state` (`pending`, `eof`, `overflow`, `read_error`, `unknown`)
and `retained_prefix_bytes`/`read_bytes` counts carrying explicit
`exact`, `lower_bound` or `unknown` semantics. A pending stream is a lower
bound, never a zero claim; an EOF-complete or overflowed stream's retained
prefix is exact; a read failure's counters are final and exact for a prefix
that was never delivered. The refused attempt's prefix is the failed-prefix
accounting: it is reported separately from the completed-capture `retained`
totals, never added to them, and never charged twice against the cumulative
caps. `leader_exit_observed` stays `unknown` unless the leader was safely
seen still running within the budget. The block appears only on transport
refusals and true pre-step budget boundaries; phase failures that are decided
about completed captures (a probe that exited nonzero or returned unusable
identity data) or about non-transport steps carry no block, so a spawned and
completed capture is never mislabeled `not_attempted`. A stream `read_error`
is not inducible through real spawned pipes; it is qualified at the
production reader plus the snapshot mapping. The block never carries bytes,
argv, environment, raw errors or digests, and equal preparation and
cumulative counts still prove nothing about a failed command's output.
Launch diagnostics on both
human and JSON surfaces exclude raw argv, command strings, configured
container/service names, output, credentials and host error payloads. The
schema-v1 `container` and `service` keys retain their shape and contain the
literal `[redacted]` on launch failure. Other failures also do not echo
captured output or credentials. Terminating the owned host client group is not
proof that a remote container process stopped:
`remote_cancellation` stays `unverified`, no remote PID is guessed and an
uncertain command is never automatically repeated.

## QA Recipe

When container shell behavior changes, rerun the two repo-owned checks below
before trusting the result.

### 1. Prove exec readiness handling

This task pins the drift path end to end:

- status reporting exposes `primary_service_exec_ready`
- runtime status warning text stays stable

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
- export, import or reset environment data
- operate a simple compose-backed repo without the fuller systems and
  workspaces wiring

If the question is instead:

- how should `dev`, `[systems]`, and `workspace` fit together?
  - use [`064`](064-system-workspace-and-dev-contract.md)
- which service inputs does a catalog support?
  - use [`067`](067-catalog-services-reference.md)

## Manifest Shapes

### Managed host listeners

Host processes that serve a browser route can declare the listener they own.
Effigy gives the listener its address and verifies the child generation's own
listening socket against it: no adapter is needed. Declare a fixed unprivileged
loopback port such as `bind = "127.0.0.1:4173"`, or declare port `0` to let
Effigy choose one: before any child launches it takes the address that process
last published (a restart or a re-`up` keeps it, so a dependent's
`internal_url` stays valid) or, on a first start, asks the OS for a free
loopback port. There is no port range behind that choice, and nothing else
records it.
A declared port is the manifest's own: it is used untouched and is never
reserved.
For disposable profiles, point `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT` at an owned
private root and start that instance with `gateway setup-tls` and `gateway up`
using the same `--private-state-root`. This keeps trust and route state scoped
to the profile.

```toml
[[containers.web.host_processes]]
name = "frontend"
run = 'exec ./scripts/start-frontend'
cwd = "."
restart = "on-failure"

[containers.web.host_processes.listener]
bind = "127.0.0.1:4173"

[containers.web.host_processes.listener.readiness]
path = "/health"
status = 200
timeout_secs = 60

[containers.web.host_processes.listener.route]
domain = "frontend.example.test"
tls = true
```

Listener routes go to the shared gateway, so browsers reach them like any
other container route. Set `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT` to publish them
to a private gateway instead (see below).

Switching between the container environment and managed host listeners needs no
manual route cleanup. When a listener claims a gateway domain that the same
checkout and scope already published as a container route, Effigy hands the
route over in one locked transaction, and the container environment may keep
running. Starting the container environment again takes the domain back once
the recorded listener generation is no longer live. Starting either
environment while a conflicting environment of the same checkout and scope is
live is one preflighted handover: Effigy first checks everything that could
refuse (foreign route claims, loopback pool capacity, manifest, policy and
ownership validation), ends every live headless managed owner bound to
the losing environment through the same path as `effigy <task> stop`
(one line names each managed task ended; any other environment's owner is
never touched) and waits for the owner's real settle: its whole stop
cascade finishes - owned processes gone, exec'd container-side groups
ended through the exec adapter, every task lock released through the
lock owner-token rules - on a derived settle budget, never a fixed
guess. The owner's cascade already brought its environment down, so
the handover verifies that and races no second conflicting down, then
brings the live environment down whole if anything is somehow still
live - host processes and listeners stopped, routes withdrawn,
containers stopped, data kept - and starts the requested one, printing
one line naming the handover. The handover refuses only when ending a
bound managed owner or stopping the live environment itself fails, and
names what is still running without advising the stop that is already
in progress; the stopped environment restarts with a plain `up`. Another checkout or
worktree generation still refuses, and stopping the old container
environment never removes the handed-over listener route.

Host processes are environment-owned kernel runs. Each
`[[containers.<env>.host_processes]]` entry is one detached run in the
checkout's environment journal, owned by its own kernel resource, with custody
of its child process group. They start after the environment's services are
ready, in `depends_on` order (a dependency must declare a `listener`), and the
declared `restart`, `restart_delay_ms`, `shutdown_signal` and
`shutdown_grace_secs` apply. `effigy container up` waits for each listener's
declared readiness and then detaches; that wait has no deadline of its own, so
an app that compiles for minutes before it binds does not fail the `up`. If the
bring-up is interrupted, the kernel stops what it started.

A managed session bound to this environment shows each host process as its own
tab, in declaration order, with a status line and its live log, next to the
session's workloads — in hybrid and container placement alike
([guide 012](012-dev-process-manager-tui.md#3-managed-contract)). A plain
`[tasks.dev]` bound to the environment needs no `concurrent` entry per host
process; the environment's declaration is the tab set.

`effigy container down`, `reset` and a session's settle stop host processes
through the kernel and report every stop, for example
`[host process] frontend stopped (run 7k2…, pid 47120, SIGTERM then SIGKILL, exit 137)`.
A process that ignores its shutdown signal is killed after its grace and named
with its PID and the signals that were sent; a process that could not be
confirmed stopped is reported as residue, never dropped. A host process that
exits is recorded with its exit code and its last log lines under
`.effigy/runtime/host-processes/<env>/<name>.log`. `effigy container status`
(and `status --json` under `host_processes`) shows each host process's run,
state, PID, last exit and listener state.

An environment started by a pre-v0.15 build can leave a `<name>.pid` record or
its `<name>.spec.json` input behind. The next `container up` or `container down`
deletes it with one line and carries on: those supervisors are gone, so a
record from one that died with a reboot never blocks an `up`.

`container up --detach --json` returns each ready listener's actual address,
internal URL, public URL, generation and state-file path. A linked worktree
or marked ephemeral clone publishes its checkout-scoped name, so the same
declared `domain` can run in several checkouts at once. The primary checkout
keeps the declared name. A listener result looks like this:

```json
{
  "name": "frontend",
  "status": "ready",
  "runtime_generation": "runtime-generation-token",
  "generation": "child-generation-token",
  "address": "127.0.0.1:4173",
  "internal_url": "http://127.0.0.1:4173",
  "public_url": "https://frontend.example.test",
  "declared_route_domain": "frontend.example.test",
  "route_domain": "frontend.example.test",
  "route_tls": true,
  "state_file": "/checkout/.effigy/runtime/host-processes/web/dev/frontend.listener.json"
}
```

Effigy sets `EFFIGY_MANAGED_HOST_LISTENER_BIND` to the listener's loopback
address (the declared one, or the one it chose for a port `0`), `HOST` and
`PORT` to that address
and port, and `EFFIGY_HOST_PROCESS_PUBLIC_URL`
and `EFFIGY_HOST_PROCESS_INTERNAL_URL` to its own route origin and listener
endpoint. It supplies `EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE`, a private path
in the process's runtime directory beside its log, and
`EFFIGY_MANAGED_HOST_LISTENER_GENERATION`, the current child generation token,
only for the adapter scripts shipped bundles already carry. Effigy never reads
that report and removes the file when the generation stops, so a new host
process needs no adapter. Any ordinary server that listens on `$HOST`/`$PORT`
is a managed listener: Effigy sends the readiness request to that address and
publishes the route when it answers.
`run = 'exec effigy profile:api'` is enough. A server that compiles for minutes
before it serves needs no adapter and no oversized timeout: while its port is
closed the wait has no deadline, and a cold build's compile time is never spent
against `readiness.timeout_secs`.

`HOST` and `PORT` are the listener's address, so they replace
an ambient `HOST`/`PORT` inherited from the shell Effigy was started in: a
server that reads them binds the port the gateway route points at. Only a value
the manifest itself declares outranks them (for example an adapter that must
see a specific `HOST`), and `EFFIGY_MANAGED_HOST_LISTENER_BIND` always carries
that address unconditionally.
The process must bind strictly to the requested address: a wildcard bind
(`0.0.0.0:<port>`) is a different address, so the port stays closed and
the wait continues until that child exits. Readiness is an
HTTP `GET` to the configured path with the effective route domain as `Host`;
only the configured status makes the listener ready.

Startup has two phases, and only the second one has a deadline. While the
listener's port is still closed and the child is alive, Effigy waits with no
deadline at all and writes a progress line to the process log about every thirty
seconds; the child exiting is what fails the generation. `readiness.timeout_secs`
starts once the port first accepts a connection and bounds reaching the
configured status from there, so a build that takes twenty minutes and a server
that answers in a second both start. A failed startup returns an error and
leaves a `failed` state instead of publishing an endpoint, and the diagnostic
names the listener address, the readiness request and the last probe outcome (or
the child's exit code). A listener address that another process already answers
fails the start naming that port.

Once published, the route stays published while the supervisor's run is alive
and the port keeps answering: Effigy probes that port every five seconds, and
three consecutive misses withdraw the route until it answers again. A miss
never restarts the child, and a dependency change replaces a dependent only
when the dependency's supervisor generation changed.

The versioned state file remains the current status source after startup.
Use `depends_on = ["frontend"]` on another host process to wait for readiness
and receive `EFFIGY_MANAGED_HOST_FRONTEND_INTERNAL_URL`,
`EFFIGY_MANAGED_HOST_FRONTEND_PUBLIC_URL`,
`EFFIGY_MANAGED_HOST_FRONTEND_STATE_FILE`, and
`EFFIGY_MANAGED_HOST_FRONTEND_GENERATION`. When the listener's supervisor
replaces its child generation, the dependent process restarts with its new
endpoint environment. Effigy withdraws the route before stopping or replacing a
child generation.

Fixed ports must bind exactly and fail on collision. Static `target_host` routes
keep their existing external-target behavior.

### Restart one host process

`effigy container <NAME> restart <process> [<process>...]` is `up`'s start path
for the processes you name. Each one's current generation is replaced, its
declared readiness is awaited and its route is published again; the
environment's containers and every other host process keep running untouched.
It is the way to act on one process - stop `farmyard-jobs` with
`effigy run stop <run>`, work through the API, then
`effigy container web restart farmyard-jobs` - without paying for a whole
`container up`. A process stopped with `run stop` is simply started again.

A process that `depends_on` a restarted process is restarted after it, in
dependency order, and named as a dependent in the output, because its readiness
and route read that dependency. A process that does not depend on a restarted
one is never touched. An unknown process name fails before anything changes. An
environment that is not up is named in one warning and the named processes start
anyway: an environment that declares only host processes has no containers to
prove. A restarted listener keeps the address it published, so a
dependent's `internal_url` keeps working. A restart holds the environment's gate
for its replace step - the stop plus the spawn - and releases it before the
readiness wait, so a `down` from another shell still stops a listener that never
answers instead of waiting out the restart's gate budget.

```
[host process] farmyard-jobs restarted (run 7k2…, listener 127.0.0.1:41873, route https://jobs.example.test)
[host process] scheduler restarted (run 91x…, dependent)
```

`--json` reports the same set under the container envelope as
`effigy.container.restart.v1`, with one `restarted` entry per process naming its
`run`, whether it was a `dependent`, and its `listener` when it declares one.

### Host process environment

A host process `env` value is literal text plus `${...}` references to facts
Effigy has already resolved for this environment. Effigy validates the names
when the container policy loads, so a typo fails at load with the offending
`env` key and the list of valid names. Values resolve when the process starts,
after Prepare assigned the published ports and after every declared service
reported ready. `$${` writes a literal `${`.

| Reference | Value |
| --- | --- |
| `${service.<name>.host}` | address the service is published on (`127.0.0.1` by default) |
| `${service.<name>.port}` | the service's published host port |
| `${route.<domain>.public_url}` | a gateway route's public origin, by declared or scoped domain (container route or managed listener) |
| `${host_process.<name>.public_url}` | a managed host listener's own public origin (its own name included) |
| `${host_process.<name>.internal_url}` | a managed host listener's loopback endpoint |
| `${domains}` | the declared domain set, comma separated |
| `${apex}` | the shared apex of the declared domains (the cookie domain) |
| `${gateway.https_port}` | the active gateway's HTTPS port |

```toml
[[containers.web.host_processes]]
name = "api"
run = 'exec ./scripts/api'
env = { DATABASE_URL = "postgres://app:secret@${service.postgres.host}:${service.postgres.port}/app",
        COOKIE_DOMAIN = ".${apex}",
        SELF = "${host_process.api.public_url}" }
```

A reference whose value is not knowable yet (a service the render publishes no
host port for, or gateway TLS that is not configured) fails when the process
starts rather than guessing.

Host processes also receive the active gateway's CA, so a host-side client can
call its own HTTPS route:

- `EFFIGY_GATEWAY_CA_FILE` is the neutral name, set whenever the CA is known.
  A private gateway (`EFFIGY_GATEWAY_PRIVATE_STATE_ROOT`) contributes
  `<root>/ca/rootCA.pem`; the shared gateway contributes mkcert's root CA.
- `NODE_EXTRA_CA_CERTS` and `SSL_CERT_FILE` are filled in for Node/Bun and for
  curl, Python and Ruby respectively. Effigy sets these only when the operator
  has not already exported them and the process does not declare them, so an
  operator trust store is never replaced.

### Readiness failures

A readiness request is one HTTP/1.1 `GET` to the configured path. Effigy reads
the status line and accepts only the configured status. A refused connection
means the app has not opened its port yet, so the wait carries on without a
deadline; missing or malformed status lines, and other statuses, mean the port
is open and the declared timeout runs. A consumer server does not need to keep
its read side open or answer in one write.

When startup fails, `<name>.listener.json` keeps a one-line `diagnostic` for
the failed generation: the listener address, the readiness path and status, and
the last probe outcome (or the port that already answered, or the child's exit
code). The diagnostic does not release, signal, or publish anything, and the
top-level `status` remains the current state. It is described in the managed
host listener section of the container runtime contract.

- declare services from the catalog; a service the catalog lacks gets a
  project fragment under `infra/dev/catalog/<name>/`
- a repo-owned `compose_file` is deprecated; migrate it (below)

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
routes = [{ domain = "my-app.test", tls = true }]

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

### Sharing service data with another environment (`data_from`)

A container environment can borrow another environment's service data volumes
with `data_from`. Services that both environments declare with the same name
and the same catalog entry mount the named environment's volumes — one
Postgres, one Silo bucket per checkout. Switching between the two
environments hands the other live environment over whole: `container up`
brings it down with data kept before starting the requested one and prints
one line naming the handover, with no refusal.

The hybrid shape from a bundle `dev` profile plus its container twin is the
canonical example:

```toml
[containers.dev]
primary_service = "app"

[containers.dev.services.app]
catalog = "workspace-rust-bun"

[containers.dev.services.db]
catalog = "postgres"

[containers.dev.services.s3]
catalog = "silo"

[containers.dev-hybrid]
primary_service = "app"
data_from = "dev"

[containers.dev-hybrid.services.app]
catalog = "workspace-rust-bun"

[containers.dev-hybrid.services.db]
catalog = "postgres"

[containers.dev-hybrid.services.s3]
catalog = "silo"
```

`dev-hybrid` mounts `dev`'s `db` and `s3` volumes for the same checkout and
worktree scope; a linked worktree borrows that worktree's volumes, never the
primary checkout's. `app` and everything else about the environment —
project name, published ports, routes — stays separate. Services that only
one side declares keep their own volumes.

Validation refuses, at manifest load, an unknown or self target, chains or
cycles, driver mismatches, a retired `compose_file` on either side,
`share_runtime_identity` on the borrower, and the same service name bound to
different catalogs.

The owner's data is protected: borrowed volumes are declared `external` in
the generated Compose, so `reset`, `reset --wipe-data`, `retire`,
`retire --dead` on the borrowing environment never deletes
or claim them. `reset --wipe-data` on the owner still removes them. An
environment without `data_from` keeps its own fresh volumes.

### Optional extra workspace mounts

Use `systems.<name>.workspaces.<workspace>.mounts` for optional sibling or
external workspace paths. The legacy string and structured forms preserve
their source, target, and option meanings:

```toml
[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
mounts = [
  "../shared:/workspace/shared:ro",
  { source = "../tooling", target = "/workspace/tooling", options = ["ro"] },
]
```

`container up` resolves these optional sources after runtime and shared
service preparation, just before it submits the primary Compose up. A source
that is absent then is skipped; a required catalog member still fails if it
is missing. Status and other policy reads do not announce a runtime skip, and
Effigy does not create the host source directory for an optional mount.

After a new create spec is submitted and Compose up succeeds, the diagnostic
reports each included or skipped source/target pair. Detached
`container up --json` adds
`workspace_extra_mounts` with `compose_up_dispatch: "succeeded"` and
`runtime_mount_verification: "not_performed"`. This records the requested
Compose spec, not proof that the runtime created or recreated a container or
applied a mount. Unreadable sources and sources that disappear during
resolution fail before Compose up.

Effigy assembles runtime Compose and assets in memory. `container render <env>`
prints their exact paths under `~/.effigy/state/objects/<digest>`. Prepare
publishes that complete immutable generation; a generated asset is named
relatively inside it, and Compose resolves that against the project directory,
so neither a container environment nor the Compose command line carries an
Effigy interpolation value. A catalog service's Dockerfile is rendered inline
(`dockerfile_inline`) from that content instead of a path, because Compose
reads `build.dockerfile` against the build context. The former mutable output
under `.effigy/runtime/compose` is replaced automatically on the next Prepare.

A shared service is rendered outside any checkout's generation, into one
machine-wide content-addressed directory under the Effigy home
(`~/.effigy/shared-services/<project>/<digest>/`) with absolute bind sources. So
every checkout that shares it hands Compose byte-identical input at the same
absolute path, and an unchanged stack is not recreated: a second `container up`
leaves every container, the shared service included, exactly where it is, and
deleting a checkout leaves the shared render's files in place. Only a service
whose rendered configuration really changed is replaced; a changed shared
service is stopped and removed before its replacement is created, so its
published port is never held by both, and its data volume is kept. One machine
gate per shared Compose project serialises checkouts whose `up`s overlap.

Treat the object directory as runtime output, not repo-owned source.

A fresh linked worktree needs no preparatory command before its first container start.

Runtime-scoped checkouts — linked Git worktrees and full clones whose local
Git config sets `effigy.runtimeScope = ephemeral` — get a stable generation
suffix on generated Compose project names (`-wt-` for a worktree, `-ec-` for
a marked clone). Containers, networks and managed volumes therefore use
different identities from the primary checkout, other worktrees and other
clones. Unpinned host ports use Compose or OS assignment. A scoped checkout
replaces fixed `host.ports` pins with OS-assigned ports unless
`stable_ports = true` opts into checkout-path-derived ports; the primary and
unmarked clones keep their declared pins unless that option is set. Repeated
commands in one scoped checkout reach the same stack; deleting and recreating
it at the same path gets a new identity. To intentionally share one Compose
stack, set
`share_runtime_identity = true` under `[containers.<name>]`.

The clone marker is read from that checkout's own `.git/config` only — never
from global or system Git configuration, and `[include]` directives are not
followed. Only the tool that created the clone (Nucleus sets this marker on
every task workspace clone) can scope it. A marked clone owns its `.git`
directory outright, so no linked-worktree shared-Git-directory mount is
added to its container.

Scoped checkouts also get a distinct gateway host map. Declared `app.test`
becomes `app-w<host-key>.test`; helpers such as `mail.app.test` stay under
that apex. Managed listener `domain` values follow the same rule: a listener
declared as `api.acme.test` publishes as `api.acme-w<host-key>.test` in a
linked worktree. The primary checkout and unmarked clones keep the declared
names. `effigy container hosts --json` is the machine-readable map for
public URLs, origins, cookie scope, WebAuthn and test selectors, and
reports scope kind `worktree` or `ephemeral-clone`. Listener-only routes
appear there with their declared and effective names. Effigy does not
rewrite application configuration. `effigy container retire --yes
effigy container retire --dead --yes` removes
that stack's containers (running, stopped, or Created), networks, mutable
volumes, isolated routes and ports, by the backend's own
`com.docker.compose.project` plus `com.effigy.checkout` and
`com.effigy.scope` labels. Shared services, external volumes, shared-identity routes and every volume
retained by `--keep-data` stay.
Nothing is recorded beside those labels: there is no
`.effigy/state/runtime-scopes/` journal, no checkout generation and no custody
fence, so deleting a checkout deletes nothing else and a re-cloned checkout at
the same path needs no reconciliation. Each container is stopped before it is
removed, so a `restart: unless-stopped` service does not come back while it is
being retired. A backend that cannot list resources, or that returns malformed
inspect output, fails instead of reporting an empty stack; a stopped Colima
profile is a warning, because nothing on it is running and it cannot be read
without starting its VM. TLS certificates for isolated routes are a cache keyed
by domain:
`down` and retirement keep them, and the gateway daemon removes a certificate
no route names once it is seven days old.

Archive callers that need the identity before the checkout disappears should
run `effigy container scope --json`. It reports the absolute checkout path,
scope kind and full generation token without loading a container policy or
contacting a backend or writing state. A scoped checkout with no prepared
runtime returns its scope kind and a null token; Prepare establishes the token
in its private Git directory. A primary checkout or unmarked clone returns kind
`none` and a null token. Persist the token before archive and use
`effigy container retire --scope <token> --yes` while its checkout exists.
After deletion, use `container retire --dead` from a live checkout.

Paseo worktree archive should call retire from `paseo.json`
`worktree.teardown` before the checkout disappears. After the directory is
gone, run `effigy container retire --dead --yes` from a live checkout (or name
one with `--repo`). Bootstrap has no teardown of its own: environments own reset and retirement, so worker
scopes are retired exactly as above. A failed retire must stay non-zero so the
leftovers stay visible and archive does not swallow residue.

Worktrees removed without running their teardown (for example Queue task
checkouts) leave their stacks behind. `effigy container retire --dead` groups
the backend's Effigy ownership labels by checkout path and retires the groups
whose path is missing. A relative or unreadable path label is reported under
`unknown` and never guessed about; a live path's stack is left alone. Without a
terminal or `--yes` it refuses and prints the stacks it would retire. Each stack
is retired independently: one that cannot be fully retired is named with the
runtime's error and makes the command exit non-zero. `--json` reports every
stack in one `effigy.container.retire-dead.v1` document. To prune on a host schedule, run
it periodically, for example from cron:

```text
0 4 * * * /usr/local/bin/effigy container retire --repo /path/to/live/checkout --dead --yes
```

Older stacks lacking verifiable generation and custody labels remain unknown.
Inspect them with the runtime’s own tooling; absence of a local record never
grants automatic deletion authority.

Generated compose binds every published port to loopback by default —
the port policy rewrites fragment entries like `"3000:3000"` into
`"127.0.0.1:<allocated-host-port>:3000"`, so dev services (dbgate,
postgres, silo, mailpit, ...) are reachable from the local machine and
the gateway's TCP aliases but not from the LAN. Opt back into public
binding per container environment:

```toml
[containers.web.host]
publish_address = "0.0.0.0"
```

### Migrating a Repo-Owned `compose_file`

`compose_file` is deprecated in v0.15. Effigy reads each Compose service as a
project catalog fragment for one release and prints a warning naming the
command. Write the fragments out once:

```sh
effigy container render linux-release   # optional: the exact render
effigy container migrate linux-release
```

`migrate` writes `infra/dev/catalog/<container>-<service>/` (`service.toml`
and `compose.fragment.yml`), replaces `compose_file` with
`[containers.<name>.services.<service>] catalog = "<fragment>"` in the manifest
file that declared it, and checks that the render did not change. Comments and
other keys stay. Delete the old Compose file afterwards; it is no longer read.
Rerunning after an interruption completes the migration.

The translated environment is a normal catalog environment:

- every service carries Effigy's checkout and scope labels, so the first `up`
  after upgrade recreates it once
- named volumes become `<project>-<volume>`; the old `<project>_<volume>`
  volumes are left alone and never adopted
- published ports follow catalog port policy: Prepare assigns the host port
  (assigned by Compose or the OS) on the publish address (`127.0.0.1` by
  default) unless `[host].ports` fixes it
- `data`, `cache` and `reset` commands apply

Compose features a fragment cannot express (custom networks, volume options,
`extends`, `$` interpolation and similar) refuse with the key to move. `eject`
is retired: generated Compose stays internal.

## Core Rules

Current v1 rules:

- repo-bound operations follow the manifest driver unless a stronger
  session-scoped override is active
- Docker/Desktop is a supported backend for bootstrap and runtime use, not just
  an ambient CLI fallback
- `primary_service` is required
- a deprecated `compose_file` must stay repo-relative
- repo-relative `host.mounts` may not escape the repo root unless they use the
  structured `external = true` form
- every volume and data action runs on the environment's exact backend and
  touches only volumes whose Effigy labels name this project; a same-named
  lookalike is reported and left alone

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
   for profile-scoped `nerdctl info` and BuildKit workers. If a running
   profile's runtime stays unavailable (for example after the VM restarted
   outside Colima's boot provisioning and came back with containerd and
   BuildKit stopped), Effigy first starts those units inside the VM and
   reports it; if that fails it refuses and names the explicit profile repair
   and reset commands
2. bring the compose environment up
3. prove the owned binding of the primary service; readiness is every
   service Running with its own health check healthy, and the runtime restarts
   a stopped service because `up` asked it for `restart: unless-stopped` once
   the stack was started
4. wait for declared environment readiness when present
5. wait for each declared gateway service port to appear on the matching
   Compose project before registering its route
6. either return immediately or attach, based on startup mode

For an explicit `effigy container profile repair` or `reset-runtime`, Effigy
tries a graceful stop and then checks the selected profile's Lima pidfiles. It
signals only an agent whose profile arguments and start identity are proven.
If evidence is missing or changed, it refuses and reports the PID and reason.
When a residual host agent requires escalation, Effigy checks `ha.pid` and
`vz.pid`. For a QEMU profile with `qemu.pid` instead, it reports the missing
`vz.pid` without signalling the QEMU driver.
Before manually using `colima stop -p <profile> --force`, verify the pidfiles
belong to that profile: Colima's force stop can signal reused PIDs. Shell and
SSH clients are never directly signalled by Effigy.

These waits are bounded. A readiness wait polls every service's own health
check and names it while it waits:
`waiting for web to report healthy (attempt 4, last: GET / 404)`. A timeout
lists each service that is not ready with its health status, failing streak and
bounded last check output, plus each exited service's exit code and last log
line, and then the next commands (`effigy container logs --service <service>`,
`effigy container status`). A stack whose unready services all fail the same
way — every check reporting `command not found`, for example — is diagnosed
after a few observations rather than waited out. `container status` shows an
unhealthy service's last check output next to its status. An environment that
declares `[containers.<name>.health]` keeps that check as its final gate.
A stopped Compose service is reported with its exit status.
`container status` lists stopped owned services instead of omitting them.
Effigy never hand-starts a stopped service and never stops or mutates
nerdctl's health-check units: the `restart: unless-stopped` policy `up` applies
through the runtime is the
restart path, and `effigy container retire --yes` / `effigy container retire
--dead --yes` removes owned mutable volumes only when asked; it is not restart
recovery. Gateway registration keeps the project, service, published-port and host-listener
ownership checks, and does not claim a route while its declared runtime
binding is missing.

Interrupted detached bring-up: a completed backend launch followed by failed
inventory, exec readiness or exposure verification leaves the scope in place.
`effigy container up --detach` exits non-zero and reports the activation as
unavailable. It never treats an unavailable probe as an empty or ready stack,
and there is nothing to clean up: the next `up` reconciles from what is running.

The supported reconciliation command is `effigy container up --detach`.
Each environment has one record (contract 053): the backend it runs on and why,
whether an explicit `up` owns it, and the runs holding it. Every `up` runs the
same one apply, and Compose recreates only the services whose rendered
configuration changed; an edit to an image, environment or mount is applied by
the next `up`, with no `down` first. A failed or interrupted `up` needs no
cleanup. Success makes the explicit `up` the persistent owner. `effigy
container down` is the explicit stop: it ends that ownership and keeps volumes,
and it refuses while a run (for example `dev`) still holds the environment.
Retirement removes owned mutable data and is not restart recovery.

JSON failure details expose `state` (`unavailable`),
`phase = "detached_activation"`, unverified readiness/exposure and supported
recovery/stop commands. `activation_stage` uses the closed vocabulary
`preparation`, `environment_handover`, `compose_activation`,
`readiness`, `runtime_integrations`, or
`persistent_owner`, `identity` names the environment and checkout, and `cause`
carries the sanitized underlying refusal (secret-shaped values redacted,
bounded). Manifest-declared detached mode
also reports preparation failures as unavailable with no recovery authority.
These stages and codes identify caller boundaries, not readiness proof; the
cause is what refused. Details contain no raw backend output, argv or
environment.
Fresh full-ID inspect must match project, service, non-oneoff running state and
checkout ownership; bindings come from current inspect published ports. An
owned replacement updates its TCP target in the same locked route transaction,
without changing foreign or shared routes. Command exit zero is not binding or
readiness proof.

Detached Compose launch sends stdout/stderr to null sinks so service writers
cannot inherit Effigy capture pipes. Failure reports command/phase and status;
use `effigy container logs` for service output. Finite inventory capture drains
both nonblocking pipes without reader threads, retains at most 1 MiB per stream,
and refuses overflow or incomplete EOF. Cancellation signals only its created
client process group and positively reaps the direct client. This does not prove
remote engine cancellation. The private-process/transport proofs do not
reconstruct historical SIGPIPE provenance or qualify real Linux/containerd
behavior; non-Unix bounded capture lacks a nonblocking adapter and refuses.

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
- on an environment this checkout never prepared whose Compose project owns no
  container, network or volume, it is a one-line no-op: `nothing to stop for
  <name>`, with `ok: true` and a `noop` reason in JSON. It renders nothing and
  calls no Compose, so the unassigned ports of a render that was never
  prepared never reach interpolation
- a stop never assigns, reserves or reclaims a port. A generation whose ports
  were never assigned (a first start that failed) is torn down with a
  teardown-only interpolation value, so its cleanup succeeds; `up` and
  `prepare` still fail closed on that render

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
Colima-only host lists its running environments without starting Docker
Desktop. Genuine failures still make the inventory incomplete and report the
backend, profile, and error once per launch.

## Task Activation

Container-backed tasks now use two distinct lifecycle models.

### Commands inside the workspace container

An Effigy invocation that already runs inside its own workspace container
(`EFFIGY_INTERNAL_CONTAINER_HANDOFF=1`, set by every compose exec into the
target service) never probes a host runtime. `container shell --command`,
`container exec`, and `exec` execute the requested command directly where
the caller already is — same working directory, inherited handoff
environment — instead of spawning `colima`, `nerdctl`, `docker`, or
`compose`, which are unreachable from inside. A bare interactive
`container shell` opens the login shell locally for the same reason. Task
routing already stays local under handoff; this closes the remaining
exec surfaces so a task invoked inside its own container runs as the
in-container execution instead of failing on host-only runtime probes.
The opt-in bounded shell (`--bounded`) takes the same local path: the
absolute deadline and stdout/stderr caps still apply to the local
execution and the same bounded-exec schema renders, with no discovery,
hold, or workspace-user fallback phases.

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
  service before dispatch; a mismatched mount stops the task before it runs.
  Changed Cargo inputs are touched inside the container so host edits
  invalidate stale build artifacts.
- public gateway/runtime route registration is reconciled when the container
  declares a gateway surface
- the task holds its environment while it runs: `container down`, a handover,
  a reset or an idle stop refuses until the task ends

Holder-based idle stop:

- a stack a task had to start is held: it stops when the last run using it
  ends. There is no timer.
- an explicit `effigy container up` that leaves the stack running makes it
  persistent: tasks borrow it and never stop it. Every `effigy container down`
  that stops the stack ends that ownership - the checkout form, `--global`,
  and the cwd-scoped form.
- the notice that announces a held stack names `effigy container up --detach`
  and `EFFIGY_DISABLE_HOST_CONTAINER_LEASE=1` as the ways to keep it running
- `effigy container status` and `effigy doctor` show each environment's
  backend, why that backend was chosen, and its record

The goal is one warm-runtime contract for non-shell tasks instead of separate
behavior for deferred versus explicit container routing.

For a fresh Bun worktree, prepare dependencies with the repo's own dependency
task before QA (normally the committed lock with `bun install
--frozen-lockfile`). Built-in tests report this gap when a selected Bun suite
has no installed dependencies, before reporting source test failures.

### Workspace Exec Identity

Host-routed tasks and `effigy exec` use the container's declared
`workspace_user` and `workspace_home` when they target the primary service.
Explicit execs against another service keep that service's configured user.
Interactive terminals retain a TTY; pipes, agents, and other non-console
callers run without one.

Plain primary-service `effigy exec` requires the primary service to already
be running. If it is down, Effigy gives the explicit `effigy container up
<NAME>` command and returns without starting the VM, service, or gateway.
`effigy dev` is also a project start path. Routed tasks keep their existing
activation lifecycle.

Headless workspace, primary-service exec, and routed tasks prepare declared
disposable Rust build/cache paths for that resolved numeric user before
children launch. Named cargo/target volumes exclusive to the primary
service are repaired and then verified with an actual read/write/create-lock
probe. Named volumes written by another compose or managed service are not
mutated: rust caches are probed only, and other shared named volumes are
refused. Sharing follows the writer set, not every reference: a service that
mounts the volume read-only is a consumer, so a primary-owned Composer
`vendor` or JS `node_modules` volume with one writable service (the app) and
read-only Nginx-style consumers stays repairable, while a volume written by
two or more services still is not. Bind-mounted rust paths are probed only;
Effigy never recursively chowns host source, siblings, or shared caches.
Isolate `target` with catalog `isolated_dirs` when the host bind is unwritable
inside the container. A matching owner at the mount root is not enough:
nested `target/debug/.cargo-build-lock` and Cargo `registry/src` /
`git/checkouts` must be usable by the resolved user, and owned dependency
volumes get the same root-and-contents repair with mount modes preserved (no
group/world write is added for read-only consumers). Anonymous colon-less
compose mounts such as `/workspace-root/api/target` stay classified as
named-volume owned-disposable rust targets.

Recursive repair also prunes every declared child mount from its parent scan,
including nested binds and volumes on the same filesystem device. A nested
host bind under an image-owned cache is checked for read/write access as the
numeric workspace user. Effigy does not compare its root-observed owner to the
workspace uid or write a probe file in the bind; a root-owned cache passes when
the numeric user's access works. An unwritable bind fails with its mount
source and guidance to grant that uid access, while its root and contents stay
untouched. Same-parent dependency trees (`vendor` beside `node_modules` as
separate volumes) are distinct mount scopes and each gets its own recursive
repair. The host-mount ruling is recorded in
[`005-container-runtime-contract.md`](../knowledge/contracts/005-container-runtime-contract.md).

Exec preparation batches metadata and numeric-user access probes for the
whole ownership plan, and scans owned paths together before repairing only
dirty volumes. Clean paths launch no repair. Each invocation still checks
current ownership and access because nested paths can change externally and
there is no exact invalidation proof for a cached result.

`container up` runs the same workspace permission preparation as an
integration stage once the owned binding is verified, before secrets,
gateway exposure, and readiness reporting. `up` is the one stage every
first creation, reuse, and handover passes through, while managed children
and `container shell` enter through exec without their own preparation —
so owned-disposable cache volumes (Cargo registry/git, target dirs) are
writable by the workspace user however the stack was created. A
preparation failure refuses closed: detached `up` leaves the started scope in
place for the next activation to reconcile, attached `up` cleans up, and
neither retries, recreates, or adopts anything.

For a running workspace, `effigy doctor` remains read-only and reports
`container.workspace-ownership` when declared cargo/target mounts, their
nested rust lock/cache paths, managed disposable volumes, Composer `vendor`
or JS `node_modules` volumes that are nonrepairable (shared between writers,
external, or mounted read-only in the primary), or
`$BUN_INSTALL/install` are unusable by the resolved workspace uid/gid.
Dependency volumes are never silently omitted: an unwritable forbidden or
shared dependency volume is a finding before any repair, and the same
fixture reads clean after the numeric user's access actually passes. The
check batches metadata reads for the known mount roots and each required
nested path depth, then checks read/write access as the resolved numeric
uid/gid for present non-symlink paths. Overlapping declared targets reuse a
sampled nested path. A sample on a mount stops its later nested probes; it
does not walk the mount. Every preliminary liveness probe (Colima `status`,
the primary service Colima/`compose ps` check, `colima ssh` socket preflight)
and every batch child converts the same remaining doctor deadline once, just
before it spawns, so none gets a fresh budget. The Colima direct-exec path
keeps that absolute deadline through uncached service discovery and the
capture it supervises, so a slow spawn cannot rebase it and an already-expired
deadline never spawns. A hung probe is killed and reaped at the deadline; an
unavailable batch discards partial permission samples and reports verification
incomplete with the sampled paths; it never reports clean or proves a
permission fault. A bounded doctor run never restarts Colima.
Stopped, unavailable, and no-workspace-user states are distinct from clean.
The finding is not a license to chown the checkout.

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
standard `data:seed` task. It currently targets the repo default
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

The staged file is cleaned up when the seed succeeds. A refusal or an aborted
import leaves it in place, so a corrected re-run does not need the source
artifact again.

### Shared database targets

A data target whose declared service is `shared = true` is removed from the
environment's own generated Compose, so `data seed` and `data dump` exec in the
shared service's own machine-wide project and render instead of the
checkout-local stack. `stack` and `hybrid` environments that declare the same
shared service identity therefore seed and dump one machine-wide database and
volume, exactly as `container up` starts one shared service. `data_from` stays a
checkout-local volume feature and never redirects a shared target.

Before the first statement Effigy proves the declared shared container is
running in the expected project, from the declared render digest and for the
declared service. A missing render, a stopped or absent container, a container
created from another render, more than one running container for the service,
or environments that declare the same service with different shared identities
all refuse with the exact project and the recovery command. A data command
never publishes a render, starts or replaces a shared service, or falls back to
a local service of the same name. Credentials reach the container by variable
name, never in the command line.

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
- `tls = true` works after `effigy gateway setup`
- when TLS is enabled, plain HTTP requests are redirected to HTTPS

TCP catalog services such as postgres, mariadb, redis, and memcached also
answer on their checkout's loopback alias address. That means host and
container code can use the same stable names without hand-written
`/etc/hosts` edits.

By default, generated TCP aliases, the `container hosts` cookie apex and
gateway service-alias registration all use the same apex.
For a linked worktree, that effective apex includes its checkout host key even
when the environment declares a base domain and managed listener but no DNS
routes. The primary keeps its declared apex.

An undeclared apex is inferred from the HTTP routes. Effigy picks the
shallowest route host served by the primary service (the service of the front
route) or by a managed host listener, and route order decides a tie at the
same depth. That keeps a bundle whose front route is a subdomain
(`s3.acme.test`) from naming that subdomain as the apex, as long as a route or
a host listener publishes the apex (`acme.test`). Only when neither the
primary service nor a managed listener serves a usable host does the
shallowest route host of any service stand in. A wildcard route never becomes
the apex, and the inferred apex is always one of the declared route hosts,
never a parent domain the routes happen to share. The resolved apex is the one
added to Compose service aliases and route-parity `extra_hosts`, and the one
the gateway registers for `postgres`/`mysql`/`redis` style service aliases.
When the inferred apex is not a suffix of every HTTP route, `container hosts`
warns, and its JSON `base_domain` object carries `"inferred": true` with the
warning.

**An undeclared base is inferred; declare `[containers.<name>.dns]
base_domain` to choose it.** Set it when the routes do not share the apex you
want, or when a single subdomain route is all Effigy can see. For example,
with routes `s3.acme.test` and `app.s3.acme.test`, setting
`base_domain = "acme.test"` gives the postgres service the alias
`postgres.acme.test` and reports `acme.test` as `cookie_domain`. A declared
value is never inferred or rewritten, and it does not change the declared HTTP
routes.

```toml
[containers.web.dns]
base_domain = "acme.test"
routes = [
  { domain = "s3.acme.test", tls = true },
  { domain = "app.s3.acme.test", tls = true },
]
```

### Operator gateway setup and control

Run `effigy gateway setup` from your normal user account once. This is the
administrator step: it installs resolver and loopback prerequisites and trusts
your existing mkcert CA. Install mkcert first. The daemon, route registration,
and ordinary `up`, `down` and `status` run as your user and never elevate.

```sh
effigy gateway setup
# Add a public development suffix explicitly when needed:
effigy gateway setup --suffix dev.example.org
effigy gateway up
effigy gateway status --json
effigy gateway down
```

`gateway up` starts the daemon when none is running, and `gateway down`
followed by `gateway up` is what replaces a running one: `container up`, `dev`
and every other path that needs the gateway only start it when it is absent and
otherwise use the running daemon, printing one warning that names both builds
when it is a different Effigy build.

Setup includes suffixes already present in the route table and the managed
`test` suffix. Re-run setup when adding a new resolver suffix. macOS receives
scoped `/etc/resolver` files and a native loopback alias boot job. If direct
loopback port 80/443 binds are denied, a root-owned, socket-activated bind
broker supplies only those listeners. Its setup-pinned operator UID is checked
with `getpeereid`; allowed addresses are `127.0.0.1` and `127.1.0.1` through
`127.1.0.50`. It transfers descriptors after the user daemon starts and drops
its copies after the daemon acknowledges adoption. It never reads routes,
certificates or user state, or serves traffic.
The helper image is copied into a root-owned private directory, rather than
executed from a user-writable installation.

Linux setup installs a systemd dummy-link split-DNS unit with no default-route
role. It applies only `CAP_NET_BIND_SERVICE` to the installed Effigy executable;
it does not lower the host-wide unprivileged-port threshold. Replacing that
binary removes its file capability, so run setup again after an executable
upgrade; `effigy gateway status --json` reports `launcher.low_port` as `missing`
until then. Foreign unit files and foreign executable capabilities are preserved
with an error. The installation path must be one real file: a symbolic or hard
link at the installed path is refused for both the unit and the capability,
because a link's target can be retargeted after setup. This adapter requires
systemd-resolved, the documented `ip` and `resolvectl` tools, and systemd's user
manager for the login launch below.

Linux setup also installs one launch-only systemd **user** unit,
`~/.config/systemd/user/effigy-gateway.service`, and enables it for the login
session. The unit runs `effigy gateway up` at login, `effigy gateway down` when
it is stopped, and nothing else. The daemon's kernel Run stays the only lifecycle
authority, so the unit declares no restart policy and never runs the daemon
itself: `systemctl --user restart effigy-gateway.service` is an ordinary
reconcile request (`down`, then `up`), and `systemctl --user stop` runs the same
`gateway down` you would. `effigy gateway down` stops the gateway without
touching the unit, so the next login starts it again; remove the unit with
`effigy gateway setup --remove`.

Beyond trusting mkcert's local CA, the administrator prompt is for the split-DNS
unit and the file capability. The installed image must also be executable by
your account, resolved the way your account's access actually resolves: your
owner execute bit when you own it, and for a root-owned image the group bits
when one of your groups is the image's group, otherwise the other bits. A
root-owned image whose group you are in but which your group cannot execute is
refused, and the launch unit that names an image you cannot run is reported
`stale` rather than healthy. The refusal names the bit that decided it — owner,
the matched group, or other — and the `chmod` that fixes that bit, with the path
a safe POSIX shell word (single-quoted, with an embedded quote escaped as
`'\''`), so `$`, backticks, backslashes and quotes in the path all arrive
literally; instead of advising an other-execute bit a matching group would
ignore. A path that is not valid UTF-8 cannot be printed as a shell word at all,
and the refusal says so rather than printing a command for a different file. An identity failure is different: a symbolic or hard link, a
foreign owner or a group/world-writable image cannot be repaired by a permission
bit, so that refusal names the install recovery (one real file owned by your
account, for example `effigy install:local` as yourself) and prescribes no
`chmod`. Everything else, including the user unit, runs as
your account. Immediately after applying the capability, setup re-execs the
installed binary as you, checks that the fresh exec carries
`CAP_NET_BIND_SERVICE`, and binds the configured low ports; a refusal fails
setup instead of leaving a gateway that cannot serve 80/443. A port a running
gateway already holds is reported as held, and the capability itself is proven
from the fresh exec's own effective set.

A user manager starts the unit at login without further action. Starting it
after boot with no logged-in user is your explicit opt-in and is never enabled
by Effigy:

```sh
loginctl enable-linger "$USER"    # opt in: start at boot without a login
loginctl disable-linger "$USER"   # undo
```

`effigy gateway status --json` reports the observed `launcher` object on Linux
(`unit`, `enabled`, `linger`, `low_port`) and a `recovery` list of exact commands
for anything that is not ready. Nothing there is inferred: an unobservable linger
state is omitted, and a foreign unit or capability is reported as `foreign`
rather than claimed. A login activation Effigy cannot trust — a foreign entry or
an unsafe activation directory — is kept as its own refusal with the move-aside
recovery and never becomes "not enabled", so `status` never tells you to enable
the unit through a path Effigy refuses. An unsafe unit *file* (a link, a
non-regular or changed file) is named as that file, not as an unrelated
directory. The unit file also has to pass the same owner and mode trust the
kernel applies: a file owned by another account or writable by another account
is reported `unsafe` and named, never `installed`, and `gateway setup --remove`
refuses it before any `systemctl` request, so the manager cannot stop a unit
Effigy will not remove.

`unit` is one of `missing`, `installed`, `foreign`, `unsafe` or `stale`. Setup
writes the unit's own `ExecStart` as quoted `"<installed effigy>" gateway up`
and `WorkingDirectory=` as the operator home written bare, because that value
takes one literal path and has no quoting rule.

An executable upgrade replaces the file, so its capability does not carry over:
run `effigy gateway setup` again, which reports `launcher.low_port: "missing"`
until it does. Setup then rewrites the owned unit to name the image it ran, and
the next login runs that command. A rollback that removes the image the unit
names, or leaves it a link, group- or world-writable, is reported as
`launcher.unit: "stale"` — never as ready — with the restore path,
`effigy gateway setup` and a `systemctl --user restart` reconcile request in
`recovery`. A stale unit is still Effigy's own file, so
`effigy gateway setup --remove` disables and removes it.

A changed unit definition disables the login activation before it writes the
new file, because the user manager may keep the definition it already loaded.
A failed or timed-out `daemon-reload` therefore leaves the unit `installed` and
not enabled with the recovery commands, rather than a new file whose link still
claims the old definition is live. The link comes back only from an `enable`
that setup asked for; an `enable` that reports a failure after writing the link
is observed as enabled, because the link is the fact and the reply is not.

If the systemd user manager cannot be reached, refuses, or does not answer
within 20 seconds, setup does not fail after writing the unit: it prints the
manager's own sentence and the exact `systemctl --user daemon-reload`, `enable`
and `start` commands that finish the enable, and keeps the installed unit. Those
`enable` commands are offered only when the activation is genuinely absent: if
the activation Effigy observes after any manager request — including the
`daemon-reload` that precedes `enable` — is foreign or unsafe, setup reports that
refusal instead, because enabling through it would hand the manager the path
Effigy refuses. The same rule covers a failed unit rewrite: its recovery never
recommends `enable` through an activation Effigy refuses, re-observes the
activation after a restore attempt that fails (a replacement during the restore
is reported as that refusal; the restore failure itself describes only what
happened and carries no command), reports an unreadable, missing or changed unit
file as what it is, and describes the activation it observed instead of claiming
it is off. `enable` appears there only for a verified absent activation with a
unit file to enable.
Removal behaves the same way: it disables, removes and unlinks the unit Effigy
owns, and a `daemon-reload` that refuses, cannot be reached or times out is then
reported as a failure naming the exact `systemctl --user daemon-reload` command
— the removal itself is done, but the manager may still serve the definition it
loaded.

The login activation directory
(`~/.config/systemd/user/default.target.wants`) must be a real directory owned
by you and not group- or world-writable, and the entry Effigy uses there must be
its own link to the owned unit. A missing unit directory is simply no
activation: a fresh installation, or one whose directory you removed before
retrying, still tears down cleanly instead of being refused. A directory
refusal's `chmod` is one quoted shell word and names the directory that actually
failed — `default.target.wants` itself, never the `effigy-gateway.service` entry
inside it — so a home containing a space cannot split the command and the mode
command cannot follow the service link. Only a mode problem gets a `chmod`: a
symlinked, non-directory or foreign-owned path is an identity failure and is
refused with the move-aside recovery instead, because a `chmod` through a link
changes its target and cannot repair an owner. Effigy verifies both before it asks the manager
to do anything: a `default.target.wants` that is a symlink to another directory,
or an entry that names another target or is not a link, is refused without
touching it and without stopping or disabling anything.

Effigy never deletes an activation entry itself. `systemctl --user disable`
(and `disable --now` on removal) is what removes it, and Effigy re-observes the
result afterwards — including when the unit started out disabled or had no
activation directory at all: if an entry is there — the manager could not run,
or something appeared or re-created it — setup refuses with the exact
`systemctl --user disable effigy-gateway.service` command and deletes nothing.
The same rule covers an upgrade: a changed unit definition disables the login
activation first, and the rewrite is refused rather than performed while that
activation remains. A name-based unlink cannot be bound to the inode Effigy
verified, so Effigy does not perform one at all: a replacement that appears at
the entry is preserved and reported, never deleted.

A repeated setup whose unit bytes are unchanged keeps the existing link and
reports `enabled` as it is even when the manager request fails — and that
failure is still reported with the recovery commands, because a link does not
prove the manager finished its own request. An `enable` that reports a failure
after writing the link is observed as enabled, with its failure and recovery
kept. If writing a changed unit file fails after the login activation was
disabled, Effigy restores that activation and says the installed route still
works — but only an activation this run disabled, and only when the restored
entry verifies as Effigy's own link; a unit that was disabled before the failure
stays disabled, and an entry that appeared meanwhile is named with its recovery
instead of being called a restore.

`effigy gateway setup --remove` stops the gateway, disables and removes the unit
it owns, removes the split-DNS unit and the file capability, and preserves
certificates, CA trust and interface addresses. A unit or capability Effigy did
not write is preserved with an error naming it.

The daemon uses a kernel Run and owned process custody. Its generation-fenced
control socket lives under `~/.effigy/gateway/runtime/`. Status has a shared
three-second observation budget and verifies ownership, custody, control socket,
HTTP readiness, HTTPS listener acceptance and DNS response. An unknown
observation is reported as unavailable; it never authorizes a stop or replacement.
HTTP/HTTPS accept readiness probes and initial TCP aliases bind before DNS
or lifecycle readiness is published.

An ordinary command encountering the old root gateway reports `setup_required`
and directs you to `effigy gateway setup`. Setup verifies and stops only the
recorded live v0.14.1 process generation, transfers the explicit legacy JSON
registries and leaf certificates to your account without changing their bytes,
and starts the operator daemon. Unknown process identity prevents migration.
URLs and loopback IPs remain unchanged. The endpoint Store is the only source of
routes and loopback aliases; the v0.14.1 JSON files are left exactly as they
were and are never read again.

### A deleted checkout's domain and address

A route is owned by its checkout path. A checkout that published container
routes and was then deleted does not block a new checkout: the next `up`
elsewhere takes the domain automatically, because the recorded checkout path no
longer exists. A checkout whose target stopped listening is treated the same
way. The takeover is logged in one line. Only a checkout that still exists and
still serves the domain refuses the write, and that message names it and the
command that stops it (`effigy container down` there).

Any route write also drops routes whose recorded checkout path is missing, for
every domain, so a deleted worktree never keeps a domain or a loopback address.
A path that is only unreadable, or that sits on a volume which is not mounted
yet, is unknown rather than missing, so its routes stay. Nothing withdraws a
route because its target is briefly down.
The same checkout path always replaces its own route, so a new run, a new
generation or a container-to-listener handover needs no repair command.
`down`, `retire` and settle remove the checkout's routes by path, listener
routes included, so `down` frees a domain immediately and `up` from another
checkout takes it at once. There is no quarantine and no withdrawn claim: the
previous `effigy gateway repair` and its `--owner-checkout` filter are retired.

Removal is an administrator action. It removes only verified setup-owned
resolver entries, the bind broker, the boot jobs and the Linux login launch
unit. Leaf certificates, mkcert CA trust and existing interface aliases are
preserved. A live alias may predate
setup, so owning the boot job does not authorize deleting that address. The
v0.14.1 `gateway up` restores its normal resolver/alias behavior and starts the
root gateway; use that binary's normal administrator prompt.

`effigy gateway setup --remove` stops the daemon and removes the platform setup
it created. There is no export and no re-import: downgrading to v0.14.1 leaves
the endpoint Store in place and the old JSON files untouched, so routes and
loopback aliases are not restored to it.

Qualification on Tom's Mac remains a separate operator-run check: after setup,
reboot, run `effigy gateway up`, start the CBS and Acowtancy workflows, and
check their recorded HTTP/HTTPS and database URLs. Confirm that status/up/down
and both workflows request no administrator access. Capture status and URL
results on the exact candidate. The private fixture proofs do not establish
cross-boot resolution or trust on a real installation.

#### Linux qualification on a clean base

Linux boot, restart and low-port qualification is also a separate, explicitly
authorised exact-candidate proof on a clean base (Ubuntu 24.04 with systemd as
PID 1, `systemd-resolved`, `util-linux`, `libcap2-bin`, `mkcert`, and a real
login session for the operator). It is not part of a source task, and these
commands are the proof to run, not a record of one. The operator identity is
`dev` (`uid=$(id -u dev)`) with home `/home/dev`; administrator actions use
`sudo` from `dev`, and no unit file may end up root-owned.

```sh
# 1. Clean base and negative control at the distro default.
id -u; id -un
sysctl net.ipv4.ip_unprivileged_port_start          # expect 1024
getcap "$(readlink -f "$(command -v effigy)")"      # expect no output
python3 -c 'import socket; socket.socket().bind(("127.0.0.1", 80))'  # expect EACCES

# 2. Setup under the native login umask, then the records it wrote.
umask 0002
effigy gateway setup                                 # approve the admin prompt
bin=$(readlink -f "$(command -v effigy)")
getcap "$bin"                                        # expect cap_net_bind_service=ep
stat -c '%a %U %n' ~/.config/systemd ~/.config/systemd/user \
  ~/.config/systemd/user/effigy-gateway.service
ls -l ~/.config/systemd/user/default.target.wants/effigy-gateway.service
stat -c '%a %U %n' "$HOME/.effigy/gateway" "$HOME/.effigy/gateway/certs"
stat -c '%a %U %n' ~/.local/share/mkcert/rootCA-key.pem      # expect 600 dev
systemctl --user is-enabled effigy-gateway.service           # expect enabled
systemd-analyze --user verify ~/.config/systemd/user/effigy-gateway.service
effigy __gateway-bind-probe 127.0.0.1:80 127.0.0.1:443         # capability_effective true
effigy gateway status --json | jq '.result.launcher'

# 3. Start, stop and restart requests against the kernel-owned lifecycle.
effigy gateway down; effigy gateway status --json | jq '.result.status'
systemctl --user restart effigy-gateway.service; effigy gateway status --json | jq '.result.status'
systemctl --user stop effigy-gateway.service; effigy gateway status --json | jq '.result.status'
systemctl --user start effigy-gateway.service

# 4. Boot persistence, only if the proof covers a login-less boot.
loginctl enable-linger "$USER"
sudo reboot
# before any login:
systemctl --user is-active effigy-gateway.service
effigy gateway status --json | jq '.result.launcher'
loginctl disable-linger "$USER"

# 5. An upgrade, a rollback and a foreign unit must be reported, not hidden.
effigy install:local
bin=$(readlink -f "$(command -v effigy)")
effigy gateway status --json | jq '.result.launcher.low_port' # expect "missing"
effigy gateway setup                                          # restores it
systemctl --user is-enabled effigy-gateway.service            # expect enabled
# A rollback that leaves the unit naming an image that moved away. Status runs
# from the moved binary itself, and the owned unit is restored afterwards:
mv "$bin" "$HOME/effigy-moved"
"$HOME/effigy-moved" gateway status --json | jq '.result.launcher.unit'  # expect "stale"
mv "$HOME/effigy-moved" "$bin"
effigy gateway setup                                          # unit names this image again
# A foreign unit is preserved with an error and its bytes unchanged:
cp ~/.config/systemd/user/effigy-gateway.service "$HOME/effigy-unit.owned"
printf '[Unit]\nDescription=foreign\n' > ~/.config/systemd/user/effigy-gateway.service
effigy gateway setup; echo $?                                 # expect non-zero
cat ~/.config/systemd/user/effigy-gateway.service             # expect the foreign bytes
mv "$HOME/effigy-unit.owned" ~/.config/systemd/user/effigy-gateway.service
systemctl --user is-enabled effigy-gateway.service            # expect enabled again

# 6. Exact cleanup of everything this proof installed.
effigy gateway setup --remove
getcap "$bin"                                                 # expect no output
test ! -e ~/.config/systemd/user/effigy-gateway.service
test ! -e ~/.config/systemd/user/default.target.wants/effigy-gateway.service
loginctl disable-linger "$USER" 2>/dev/null || true
systemctl --user daemon-reload
```

Destroy the VM afterwards instead of reusing its mkcert CA, trust store or
endpoint Store. Capture the candidate identity, the commands above, and each
result on the exact candidate; a passing source test is not this proof.

### Private gateway for disposable consumers

Use private mode when a disposable checkout needs real HTTPS without changing
the user's trust store, resolver files, host aliases, or privilege state. The
state root must already exist as a canonical directory owned by the current
user with mode `0700`; Effigy creates its `ca/` and `certs/` subdirectories
there. Private mode currently requires Unix ownership and permission checks;
other platforms fail closed. Keep `ca/rootCA-key.pem` private. Share only
`ca/rootCA.pem` with a client that should trust this fixture.

```sh
gateway_root=$(cd "$(mktemp -d)" && pwd -P)
chmod 700 "$gateway_root"
export EFFIGY_GATEWAY_PRIVATE_STATE_ROOT="$gateway_root"

# Create a fixture CA and localhost certificate without installing trust.
effigy gateway setup-tls --private-state-root "$gateway_root" --json

# Normal container and managed startup inherit the same private state root.
effigy container up

# Capture the OS-assigned listeners and the CA path for this instance.
effigy gateway status --json
```

Private TLS uses an absolute `EFFIGY_GATEWAY_MKCERT_BIN` executable when it is
set; otherwise it searches Effigy's bounded trusted install prefixes. Set it
only to a trusted mkcert executable. Effigy gives certificate generation the
fixture's `ca/` directory as `CAROOT` and never runs `mkcert -install` in
private mode. An invalid explicit path fails closed instead of selecting a
different executable.

Private startup binds DNS, HTTP, and HTTPS only to loopback. Each address can
be pinned with `--dns-addr`, `--proxy-addr`, or `--https-addr` on `gateway up`,
or with `EFFIGY_GATEWAY_DNS_ADDR`, `EFFIGY_GATEWAY_PROXY_ADDR`, and
`EFFIGY_GATEWAY_HTTPS_ADDR`. Port `0` asks the OS to allocate a port; status
and `up --json` report the address actually held. Do not assume an adapter
using a fixed port has migrated to this mode.

Clients explicitly trust the fixture CA and resolve the route name to the
reported HTTPS loopback address. This example uses the domain from the
consumer's TLS route; `--resolve` also sends that name for TLS verification
and SNI:

```sh
https_addr=$(effigy gateway status --json | jq -r '.result.https_addr')
https_port=${https_addr##*:}
curl --cacert "$gateway_root/ca/rootCA.pem" \
  --resolve "app.example.test:$https_port:127.0.0.1" \
  "https://app.example.test:$https_port/"

effigy gateway down --private-state-root "$gateway_root"
```

`setup-tls --json` reports `private: true`, `client_trust: "explicit_fixture_ca"`
and `client_ca_file`; private `up` and `status` report `private: true`, the
actual listener addresses, and the same CA path. A status payload has this
shape (values vary per fixture):

```json
{
  "schema": "effigy.gateway.status.v1",
  "schema_version": 1,
  "ok": true,
  "running": true,
  "pid": 48125,
  "dns_addr": "127.0.0.1:53421",
  "proxy_addr": "127.0.0.1:53422",
  "https_addr": "127.0.0.1:53423",
  "client_ca_file": "/tmp/private-gateway/ca/rootCA.pem",
  "private": true,
  "gateway_dir": "/tmp/private-gateway",
  "route_table_trust": "trusted",
  "route_count": 1
}
```

Run `gateway down --private-state-root "$gateway_root"` before removing a
fixture root. Private recovery refuses ambiguous identity instead of
falling back to operator state. The normal operator gateway remains the
default and keeps its existing setup behavior.

Scoped checkouts — linked worktrees and marked ephemeral clones — register
the effective host map from `effigy container hosts`, not the raw declared
domains. If another checkout that still serves a name owns it, registration
fails and names that checkout. This also protects TLS certificates and TCP
service aliases: stopping one scope never removes a sibling's current route.
A route from a deleted checkout or a checkout whose target stopped listening is
reclaimed on the next registration.

### Route-table trust

The operator daemon reads routes from the gateway endpoint Store in
`~/.effigy/state/machine.db`. Registration and daemon reload use committed
Store revisions; there is no legacy JSON import and no file provenance check,
because the Store is the whole authority. An Unknown Store read preserves the
last applied table and reports the observation failure. The storage and lifetime model is in
[architecture 035](../knowledge/architecture/035-gateway-endpoint-store.md);
the legacy trust model is in
[`033-gateway-route-table-trust-contract.md`](../knowledge/contracts/033-gateway-route-table-trust-contract.md).

Status never requests administrator verification. A legacy root-owned or
unknown identity directs the operator to setup, preserving its records until
setup can establish process ownership. A responding endpoint or decimal PID
alone never authorizes a stop. To restart only a repository's managed session
while keeping the gateway available, use the task-local sequence in
[`012-dev-process-manager-tui.md`](012-dev-process-manager-tui.md#restart-the-managed-dev-session).

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
while `nerdctl compose up` returns success.

Cause: a Compose service with no restart policy is never restarted by the
runtime. `up` gives every service whose stack declares no policy of its own
`restart: unless-stopped` through the runtime's own `update`, once the stack is
started, so a service whose stack an installed Effigy brought up comes back by
itself after a VM restart, a Colima restart or a crash. A service that declares
`restart: "no"`, or a stack brought up by an older Effigy, keeps the policy it
has.

Fix: run `effigy container up`. `up` applies the policy through the runtime, and
the runtime restarts the service. Effigy starts no container by hand, and it
issues no `systemctl` command on nerdctl's health-check units. If a service
stays stopped, the error is the readiness failure that names the service and
its own health check; diagnose it with:

```bash
effigy container status                 # live row, including stopped services
effigy container logs --service <name>
```

`effigy container status` shows the live `Exited` row rather than an empty
or assumed-running service list.

Under host saturation Effigy distinguishes the machine from the service: an
owned inspect that times out while the host is saturated is retried with the
same bound. When a bounded call still cannot answer, the error names the
observed host load (`load average 40.00 over 18 CPU(s)`) and says the service's
state is unproven instead of blaming it. `Exited (141)` is reported as SIGPIPE —
a closed stdout/stderr pipe — not as a health verdict; read the service log for
the writer that closed. Detached `container up` writes backend output to one
retained file, so Effigy holds no pipe a service could lose, and the original
nerdctl reader was never identified. Let the host quiesce and retry once rather
than widening timeouts or recreating the stack.

## Troubleshooting: host `docker` cannot see a Colima container

Symptom: the stack is up (`effigy container status` lists every service
`Up`), but on the host

```
$ docker inspect <container>
Error: No such object: <container>
```

Cause: the Docker CLI talks to whatever context it is pointed at. A host with
Docker Desktop installed usually has `context = desktop-linux`, which is a
different engine from the Colima profile Effigy manages (`effigy` by default).
`docker ps`, `docker inspect` and `docker compose` therefore see Docker
Desktop's containers, never the ones running in the Colima VM. Effigy's own
commands select the profile and backend from the manifest, not from the host
Docker context, so they see the real stack.

This is a context boundary, not a missing or failed container. Do not restart,
recreate or reset the environment because host `docker` cannot see it.

Use the supported per-service route instead:

```bash
effigy container status                 # every service, with its live status
effigy container logs --service db      # one service's log
effigy container logs --service db --follow
effigy container shell --service db --command "psql -U postgres"
effigy container hosts                  # this checkout's HTTP/TCP host map
```

`effigy doctor` prints the host's `docker-context` and warns when it differs
from the backend Effigy will use for the declared Colima containers. To inspect
the VM directly, address the profile explicitly:
`colima nerdctl --profile effigy -- inspect <container>`. Do not point the host
`docker` context at Colima while other projects rely on Docker Desktop.

## Troubleshooting: `/run` inodes exhausted by nerdctl health-check pid files

Symptom: every `exec` and container create in the profile starts failing with

```
no space left on device
```

even though the disk has room, and `colima ssh --profile <profile> -- df -i /run`
shows `IUse%` at or near 100%.

Cause: nerdctl 2.3.1 starts each health check as a containerd exec process and
never deletes it, so the VM keeps one pid file per check under
`/run/containerd/io.containerd.runtime.v2.task/<namespace>/<container>/`. A
busy stack leaks tens of thousands of `health-check-<id>.pid` files per
container per day, and `/run` is a tmpfs with a finite inode count. It is a
runtime defect, not an Effigy or application failure. nerdctl **2.4.1** deletes
the exec process (`process.Delete` in a `defer`, added after 2.3.1), which
stops the leak at the source. The VM's nerdctl is whatever the Colima image
ships: Colima 0.10.3 ships nerdctl 2.3.1, so check it with
`colima ssh --profile <profile> -- nerdctl --version` and upgrade Colima (or
the VM's nerdctl) for the fix.

Effigy prunes it:

```bash
effigy container profile prune-health-pid-files --profile effigy
```

`container up` and a task that starts an environment run the same bounded prune
automatically and say nothing about it in a normal run; a prune that could not
run warns once. No other command runs the prune. The prune removes only files
whose name matches exactly `health-check-*.pid`, that sit directly inside a
containerd runtime-v2 task directory, and that are older than ten minutes; a
younger file may belong to a check still in flight, so it is kept. Nothing else
under `/run` is read or removed. The command reports the counts before and
after, and `--json` emits `effigy.container.profile-prune-health-pid-files.v1`
with those counts.

Do not delete other files under `/run/containerd/io.containerd.runtime.v2.task`
or remove a container's task directory: those belong to the runtime, not to
Effigy. Pruning keeps a profile usable until its nerdctl is upgraded to 2.4.1
or later inside the VM.

## Troubleshooting: Colima VM DNS proxy timeouts during image pulls

Symptom: a `nerdctl build` of a catalog image (for example
`workspace-rust-bun`) fails after a few minutes with

```
lookup registry-1.docker.io on 192.168.5.3:53: read udp ...: i/o timeout
```

while the same name resolves on the macOS host in milliseconds.

Cause: Colima's VM resolver is a DNS proxy at `192.168.5.3` that forwards to
the host resolver. Under load or after a network change that proxy can stop
answering for a while even though the host resolves normally. It is not a
registry-credential or repository problem.

Fix: retry the pull or image build a bounded number of times, and confirm the
proxy recovered rather than changing the catalog or credentials:

```bash
# Two bounded attempts; each one is a fresh pull, not a longer timeout.
colima nerdctl --profile <profile> -- pull <image> || \
  { sleep 30; colima nerdctl --profile <profile> -- pull <image>; }
```

A second timeout means the VM resolver is still unavailable: restart the
profile (`colima restart <profile>`) and retry once. Do not treat one DNS
proxy timeout as a failed catalog image or a wrong pull credential, and do not
widen the build timeout to hide it.

## Host Runtime Fallback

On hosts with Docker CLI installed, Effigy uses `docker compose`.

When Colima exists but `docker` is not on `PATH`, Effigy falls back to:

- `colima nerdctl -- compose`

with the profile started under `containerd`.

### A deleted or stopped profile is recreated

Deleting a profile is recoverable, and it needs no manual step. The next Effigy
command that needs the profile creates it (with Effigy's managed arguments) or
starts it, and reconciles the containers the deletion left behind:

```bash
# Both of these are enough; there is nothing to start by hand afterwards.
colima delete --profile effigy-release
effigy dev                       # or any command that needs the profile
```

Every path that needs an Effigy-managed profile uses one helper:
`effigy` for stacks, `effigy-release` for the Linux workspace artifact builder,
and any other profile a container declaration names. The helper:

- creates the profile when `colima list` does not have it, and says so in one
  line (`creating Colima profile ... (first use, or it was deleted)`);
- starts it with the managed arguments when it is stopped;
- never stops or restarts a *running* profile: other checkouts' stacks share
  it. When a required managed setting drifted, for example a profile started
  outside Effigy with `forwardAgent: false`, it writes Effigy's managed settings
  and warns with the deliberate command `colima restart <profile>`;
- names what the deletion lost after it recreates the profile: that profile's
  containers, images and volumes are gone, and Effigy rebuilds the containers
  the run needs.

Creation never deletes the profile itself. `effigy container profile
purge --yes` remains the explicit way to remove one and keep it removed until
something needs it again.

An owned container whose canonical identity can no longer be read (what a
profile creation or restart leaves behind) is stopped and recreated on the next
activation, under the same exact-owner checks as every other transition: the
project, service, checkout and scope labels must all name this owner. A foreign
or unknown owner still refuses, and no check is relaxed for a container this
checkout cannot prove it owns. A run that reconciles such containers says how
many it removed.

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

## Host ports

Generated container ports are assigned by Compose or the OS. A generated
service publishes `127.0.0.1::<container-port>`, and `up` reads the assigned
host port back from the running container into its route. Nothing is reserved
between runs, so two worktrees of one project come up together and `down`
followed by `up` is immediate. A manifest pin in `[containers.<NAME>.host].ports`
is used as written; a collision fails naming the process that holds it.
`[containers.<NAME>.host].stable_ports = true` derives one port per published
container port from the checkout path, never records and never probes it, so an
unchanged stack renders the same port on every `up`.

## Loopback aliases

Each checkout holds one address in the bounded `127.1.0.1`-`127.1.0.50`
loopback pool, and its aliased routes answer with it. The route table is the
only record: an address is in use exactly when some route references it, as its
`dns_ip` or as its target. `up` gives a checkout the address its routes already
answer with, or the lowest address no route references, and `down` and
retirement free it by removing the routes. A deleted worktree's routes are
dropped by the next route write, so its address is reused without a repair
command, and a live checkout keeps its address even while its environment is
stopped.

## Current Limits

Still intentionally narrow:

- no broad multi-driver abstraction
- no richer per-service health DSL yet

## Related

- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](055-everyday-workflows.md)
- [`064-system-workspace-and-dev-contract.md`](064-system-workspace-and-dev-contract.md)
- [`072-artifact-commands-guide.md`](072-artifact-commands-guide.md)
- [`014-artifact-substrate-contract.md`](../knowledge/contracts/014-artifact-substrate-contract.md)

## Next Step

Use this page when deciding whether a repo should stay on direct
`effigy container ...` commands or move to the broader systems and
`workspace` model.
