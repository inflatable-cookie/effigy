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
worktree stacks. `container status --json` reports `mount_drift.state`
(`clean`, `drift`, `unavailable` or `stopped`) and `details`; text status and
doctor also report mismatches. Running alone does not establish mount readiness.
The next routed-task preparation or `container up --detach` applies the current
render when verified live mounts differ. Live task holders still prevent
recreation. Finish those runs before reconciling. An unavailable observation
preserves reuse and checks permissions without ownership or mode repair; an
actual access failure still blocks readiness. It does not authorize recreation,
and permission preparation refuses mismatched
named mounts before any ownership repair. Effigy never repairs a host bind by
recursively changing its ownership.

An isolated volume overlays a directory inside the checkout, so that host
mount point must exist for the runtime to mount over it. The mount observation
includes it: `container status` and doctor report a missing host directory as
`mount_drift` without creating it. `container up` and routed-task preparation
then recreate it before creating or reusing the stack, and because the missing
directory is already drift, routed-task preparation reconciles through the
guarded bring-up instead of an idempotent re-apply. A host cleanup that deletes
`target` or `node_modules` under a running stack is therefore healed: the stack
is reconciled instead of reused, and the recreated directory never replaces a
file or follows a symlink.


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
- `retire --yes` removes only this runtime scope's owned resources; `--scope <TOKEN>` retries after the checkout directory is gone

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
Effigy reserves the address, finds the listening socket in the child generation
itself, and verifies it: no adapter is needed. Port `0` asks Effigy for a
concrete port: before anything launches, Effigy reserves one loopback port in
the environment's generated range through the gateway endpoint store, keeps
that reservation across supervisor restarts within the run, and releases it
when the environment settles (`down`, `reset`, session settle). Separate
checkouts therefore never coordinate ports by hand and never race the OS for
one. A fixed port is the manifest's own and is never reserved.
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
bind = "127.0.0.1:0"

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
refuse (foreign route claims, loopback allocation, manifest, policy and
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
declared readiness and then detaches; if the bring-up is interrupted, the
kernel stops what it started.

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

An environment started by a pre-v0.15 build still has a PID-file supervisor.
The next `container up` or `container down` stops it by the exact identities in
its PID and listener records (never by a process-name pattern), withdraws its
route and removes the record before starting the kernel-run generation.

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

Effigy sets `EFFIGY_MANAGED_HOST_LISTENER_BIND` to the exact loopback address
(the reserved port for a `:0` declaration), `HOST` and `PORT` to that address
and port, `EFFIGY_HOST_PROCESS_PUBLIC_URL`
and `EFFIGY_HOST_PROCESS_INTERNAL_URL` to its own route origin and reserved
endpoint, `EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE` to a private report path,
and `EFFIGY_MANAGED_HOST_LISTENER_GENERATION` to the current child generation.
Any ordinary server that listens on `$HOST`/`$PORT` is a managed listener, with
no language-specific adapter and no report: Effigy looks for a listening socket
at exactly the address it reserved among the child generation's process group
and descendants, proves no other same-user process can also accept on it, and
then sends the readiness request. `run = 'exec effigy profile:api'` is enough
(effigy#334).

`HOST` and `PORT` are the address Effigy reserved and verified, so they replace
an ambient `HOST`/`PORT` inherited from the shell Effigy was started in: a
server that reads them binds the port the gateway route points at. Only a value
the manifest itself declares outranks them (for example an adapter that must
see a specific `HOST`), and `EFFIGY_MANAGED_HOST_LISTENER_BIND` always carries
the reserved address unconditionally.
The process must bind strictly to the requested address: a wildcard bind
(`0.0.0.0:<port>`) is a different address and fails startup. Readiness is an
HTTP `GET` to the configured path with the effective route domain as `Host`;
only the configured status makes the listener ready. A failed or timed-out
startup returns an error and leaves a `failed` or held `unavailable` state
instead of publishing an endpoint, and the diagnostic names the reserved
address (`expected_address`) and every other address the generation served
(`observed_addresses`). A reserved address that is held by a process outside
the child generation is never accepted; startup fails naming that process.

The versioned state file remains the current status source after startup.
Use `depends_on = ["frontend"]` on another host process to wait for readiness
and receive `EFFIGY_MANAGED_HOST_FRONTEND_INTERNAL_URL`,
`EFFIGY_MANAGED_HOST_FRONTEND_PUBLIC_URL`,
`EFFIGY_MANAGED_HOST_FRONTEND_STATE_FILE`, and
`EFFIGY_MANAGED_HOST_FRONTEND_GENERATION`. When the listener restarts, the
dependent process restarts with its new endpoint environment. Effigy checks
actual process and socket ownership before publishing the gateway route and
withdraws it before stopping or replacing a child generation.

An adapter report is optional. A wrapper that needs to tell Effigy which socket
it bound can write one to the path in
`EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE`, echoing the exact supplied
generation token and the address that is actually bound. Write the complete
versioned document before exposing readiness to Effigy:

```json
{
  "schema": "effigy.managed.host-listener-report.v1",
  "generation": "child-generation-token",
  "address": "127.0.0.1:4173"
}
```

When a report exists Effigy uses it as a claim it still verifies, and it must
agree with the reserved address and with the socket the generation is serving.
A listener that writes no report is verified by observation alone.

Fixed ports must bind exactly and fail on collision. Ownership proof is
currently supported on Linux and macOS; other platforms refuse when the
process or socket owner cannot be established. Static `target_host` routes
keep their existing external-target behavior.

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
| `${host_process.<name>.internal_url}` | a managed host listener's reserved loopback endpoint |
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
the status line and accepts only the configured status. Connection failures,
missing or malformed status lines, and other statuses keep the listener not
ready. A consumer server does not need to keep its read side open or answer in
one write.

When startup fails, `<name>.listener.json` keeps a `diagnostic` object for the
failed generation. It records the last observation for each phase: report,
ownership, HTTP probe status and route. Unobserved phases show `not_reached` or
`not_attempted`. The diagnostic does not release, signal, or publish anything,
and the top-level `status` remains the current state. The field list is in
the managed host listener section of the container runtime contract.

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
Postgres, one MinIO bucket per checkout. Switching between the two
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
catalog = "minio"

[containers.dev-hybrid]
primary_service = "app"
data_from = "dev"

[containers.dev-hybrid.services.app]
catalog = "workspace-rust-bun"

[containers.dev-hybrid.services.db]
catalog = "postgres"

[containers.dev-hybrid.services.s3]
catalog = "minio"
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
`retire --dead` and `ports reclaim` on the borrowing environment never delete
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
publishes that complete immutable generation. The former mutable output under
`.effigy/runtime/compose` is replaced automatically on the next Prepare.

Treat the object directory as runtime output, not repo-owned source.

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
that apex. Managed listener `domain` values follow the same rule: a listener
declared as `api.acme.test` publishes as `api.acme-w<host-key>.test` in a
linked worktree. The primary checkout and unmarked clones keep the declared
names. `effigy container hosts --json` is the machine-readable map for
public URLs, origins, cookie scope, WebAuthn and test selectors, and
reports scope kind `worktree` or `ephemeral-clone`. Listener-only routes
appear there with their declared and effective names. Effigy does not
rewrite application configuration. `effigy container retire --yes` removes
that scope's owned containers (running, stopped, or Created), networks,
disposable caches, mutable volumes, isolated routes and ports using
`com.effigy.scope`, `com.effigy.project`, or the exact Compose project
label, and only on the runtime profile that produced each observation.
Shared services, persistent volumes, external volumes, and shared-identity
routes stay. Repo-owned Compose volumes stay unless labelled
`com.effigy.persist=false`. The checkout keeps its scope inventory under
`.effigy/state/runtime-scopes/`, with journalled atomic edits and corrupt-input
refusal. Deleting a checkout deletes that inventory; no machine index survives.
A backend that cannot list resources, or that returns malformed inspect output, fails instead of
reporting an empty stack. TLS certificates for isolated routes are removed
under the route lock after an owner check; failure leaves the route and a
pending-cert list on the record. A pending certificate whose domain now has
a foreign owner stays.

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
durable record remains and archive does not swallow residue.

Worktrees removed without running their teardown (for example Queue task
checkouts) leave their stacks behind. `effigy container retire --dead` retires
scopes discovered on the selected backend whose native checkout generation is
positively gone. Backend labels and surviving shared endpoint/port facts supply
ownership; the exact recorded custody lease must be free. Missing/replaced
lease state, unavailable volumes and ambiguous ownership stay protected. Retry
progress is kept in the invoking live checkout. Without a terminal or `--yes` it refuses and prints the scopes it would retire. Each scope
is retired independently: a scope that cannot be fully retired keeps its record
for a retry and makes the command exit non-zero. `--json` reports every scope in
one `effigy.container.retire-dead.v1` document. To prune on a host schedule, run
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
postgres, minio, mailpit, ...) are reachable from the local machine and
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
  (see `effigy container ports status`) on the publish address (`127.0.0.1` by
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
   reports it; only if that fails does it repair the profile once by
   restarting it
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
A stopped Compose service is reported with its exit status. Effigy
never starts a container that is already running or that the backend is
still acting on: immediately before each recovery start it inspects that one
container and starts it only when the observation proves a stopped
`created`, `exited` or `dead` container. After a Colima VM restart, a
successful owned-container start that reports a nerdctl health-check timer
collision records a warning. If start reports that collision but the
container ends up running, Effigy records the collision line and continues
to the normal readiness and health wait, which proves or fails health
itself; it is not reported as stale backend state. If the container stays
Exited with that collision, Effigy inspects the full container ID and, when
the unit pair is an exact-owned transient health-check timer/service in the selected
profile, stops `{id}.timer`, `reset-failed` `{id}.service`/`{id}.timer`,
retries start once, and proves readiness by inspect. Persistent, foreign,
one-off, running, or unverified units are left alone; the error then names
the inspected identity, the refusal reason, and
`colima nerdctl --profile <profile> -- start <container>`.
`container status` lists stopped owned services instead of omitting them.
If the same-ID start still fails with a timer collision, Effigy refuses
service-only recreation. The current recovery interface reads inspect mount
references, but has no immutable declaration-to-mount association, exclusive
volume/data ownership proof, or supported backend preservation plan. Those
references cannot establish that recreation will retain data, including
anonymous, shared, unknown or changed mount associations. No container is
recreated and no volume is removed. `effigy container retire --yes` removes
owned mutable volumes; it is not restart recovery. The refusal is exercised
through private fake command transports with file preservation markers; this
does not qualify a real Linux/containerd health timer or a live exec session.
Gateway registration keeps the project, service, published-port and host-listener
ownership checks, and does not claim a route while its declared runtime
binding is missing.

Partial detached bring-up: a completed backend launch followed by failed
inventory, exec readiness or exposure verification leaves the scope in place.
`effigy container up --detach` exits non-zero and reports partial or unavailable
activation. It never treats an unavailable probe as an empty or ready stack.

The supported reconciliation command is `effigy container up --detach`.
Each environment has one record (contract 053): the generation it applied,
the backend it runs on and why, whether an explicit `up` owns it, and the
runs holding it. A bring-up that applied its Compose change but did not finish
leaves a partial generation; rerunning `up --detach` with the same inputs
reconciles it without recreating running services, and changed inputs refuse
until `effigy container down`. A warm owned stack reconciles fresh inspected
bindings without Compose up or primary restart. Exec readiness failures and
interrupts retain unverified state without teardown. Success makes the
explicit `up` the persistent owner. `effigy container down` is the explicit
stop: it ends that ownership and keeps volumes, and it refuses while a run
(for example `dev`) still holds the environment. Retirement removes owned
mutable data and is not restart recovery.

JSON failure details expose `state` (`partial` or `unavailable`),
`phase = "detached_activation"`, unverified readiness/exposure and supported
recovery/stop commands. `activation_stage` uses the closed vocabulary
`preparation`, `environment_handover`, `compose_activation`,
`stopped_service_recovery`, `readiness`, `runtime_integrations`, or
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
Colima-only host reclaims stale isolated loopback assignments without starting
Docker Desktop. Genuine failures still make the inventory incomplete, preserve
every uncertain assignment, and report the backend, profile, and error once
per launch.

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
integration stage once exec readiness is verified, before secrets,
gateway exposure, and readiness reporting. `up` is the one stage every
first creation, reuse, and handover passes through, while managed children
and `container shell` enter through exec without their own preparation —
so owned-disposable cache volumes (Cargo registry/git, target dirs) are
writable by the workspace user however the stack was created. A
preparation failure refuses closed: detached `up` records a partial
activation without tearing the started scope down, attached `up` cleans
up, and neither retries, recreates, or adopts anything.

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

TCP catalog services such as postgres, mariadb, redis, and memcached also get
deterministic loopback aliases. That means host and container code can use the
same stable names without hand-written `/etc/hosts` edits.

By default, generated TCP aliases, the `container hosts` cookie apex and
gateway service-alias registration all use the same apex.

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
upgrade. Foreign unit files and foreign executable capabilities are preserved
with an error. This adapter requires systemd-resolved and the documented `ip`
and `resolvectl` tools.

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
URLs, loopback IPs, generated 100-port ranges and pins remain unchanged.
The endpoint Store owns their imported state; the legacy JSON files remain
unchanged until an explicit downgrade export.

### Recover a deleted checkout's gateway claims

A checkout that published container routes and was then deleted keeps those
routes and their reserved domains: the hostnames answer from nothing, and a new
checkout of the same project cannot claim them. `effigy gateway repair` is the
supported repair. Preview first, narrowed to the exact deleted checkout:

```sh
effigy gateway repair --owner-checkout /Users/me/.t3/worktrees/old-checkout
```

The preview lists every loopback assignment, container route (DNS alias or HTTP)
and withdrawn domain claim with `reclaim`, `hold` or `retain` and a reason, and
prints a plan digest. Reclaim means the repair proved the owner gone: the path is
definitely missing, and no runtime-scope record, container in any state, live
listener or foreign owner still holds the entry. `hold` means the proof is
incomplete or something may still own it, and the reason says which; `retain`
means a live owner keeps it. Nothing is written by a preview.

Apply exactly what you reviewed:

```sh
effigy gateway repair --owner-checkout /Users/me/.t3/worktrees/old-checkout   --yes --expect-plan <DIGEST>
```

Apply refuses when the recomputed plan differs, when a successor published the
domain between review and apply, or when the release would take a live
owner. A successful apply releases the assignment, its container routes and its
withdrawn claims in one endpoint Store publication, writes a pre-edit backup with
SHA-256 evidence under `~/.effigy/gateway/`, and never touches containers,
volumes, data, certificates or another project's routes. The recovered domains
leave the endpoint reuse quarantine after about three minutes; retry the
consumer until it claims them.

When a managed listener (host process) wants a domain that a deleted checkout
still claims, its startup refuses and names that checkout plus this same command
instead of calling the claim a live foreign owner. The refusal is expected: run
the repair, then start the environment again.

For a reversible downgrade, stop dependent consumers first, then use the new
binary to remove its setup and export its latest endpoint state:

```sh
/path/to/v0.15/effigy gateway setup --remove
/path/to/v0.14.1/effigy gateway up
```

The first command stops the user daemon, freezes endpoint edits, and exports
`~/.effigy/gateway/routes.json`, `~/.effigy/gateway/loopback-ips.json` and
`~/.effigy/ports.json`. Original mappings and unknown fields are preserved,
including routes, addresses and pins added under v0.15. Unchanged documents
retain their exact bytes. There are no routine dual writes: the JSON files
become authoritative again only at this explicit handoff. Do not run both
versions' consumers during the transition.

Removal is an administrator action. It removes only verified setup-owned
resolver entries, the bind broker and boot jobs. Leaf certificates, mkcert CA
trust and existing interface aliases are preserved. A live alias may predate
setup, so owning the boot job does not authorize deleting that address. The
v0.14.1 `gateway up` restores its normal resolver/alias behavior and starts the
root gateway; use that binary's normal administrator prompt.

If export reports a changed legacy file, it preserves the file and names its
exact path; reconcile it before retrying. An interrupted export freezes v0.15
edits: rerun `gateway setup --remove` to complete it. Retrying a completed
export never overwrites edits subsequently made by v0.14.1. Active managed
listener domains are listed and refused because v0.14.1 cannot enforce their
generation fences; stop those v0.15 consumers to withdraw their routes first.
Inactive persistent reservations are not published as active legacy routes.

To return to v0.15, stop v0.14.1 consumers and run the new binary's
`effigy gateway setup`. Setup authenticates and stops the old root daemon,
restores its platform setup, and re-imports the current JSON files, including
changes made while downgraded. Unknown ownership or a remaining PID record
refuses re-import. Ordinary v0.15 endpoint edits and daemon startup remain
blocked after export until setup completes this re-import.

`proof:gateway:downgrade` prepares a fresh HOME, performs the Store round trip,
then runs the checksum-pinned released v0.14.1 Linux root gateway inside a fresh
Colima VM/container. It checks HTTP routes, a DNS/TCP alias and every exported
mapping without using the host gateway. Artifacts and the stopped private VM
remain under the printed temporary directory. This proof does not establish
macOS launchd activation or cross-boot behavior on Tom's installation.

Qualification on Tom's Mac remains a separate operator-run check: after setup,
reboot, run `effigy gateway up`, start the CBS and Acowtancy workflows, and
check their recorded HTTP/HTTPS and database URLs. Confirm that status/up/down
and both workflows request no administrator access. Capture status and URL
results on the exact candidate. The private fixture proofs do not establish
cross-boot resolution or trust on a real installation.

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
domains. If another live checkout already owns a name, registration fails
and names the owning checkout. This also protects TLS certificates and TCP
service aliases: stopping one scope never removes a sibling's current
route. A route from a retired checkout generation can be reclaimed on the
next registration. `effigy gateway repair --yes` can clean stale TCP alias
conflicts, but leaves live checkout routes alone.

### Route-table trust

The operator daemon reads routes from the gateway endpoint Store in
`~/.effigy/state/machine.db`. The first gateway mutation automatically imports
`routes.json`, `loopback-ips.json` and `ports.json`, preserving their mappings
and leaving the original files untouched as backups. Registration and daemon
reload use committed Store revisions after import; editing a backup does not
change routing. An Unknown Store read preserves the last applied table and
reports the observation failure. Legacy JSON provenance checks apply before
import. The storage and lifetime model is in
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
while `nerdctl compose up` returns success. Manual `nerdctl start` works and
may log `Unit <id>.timer was already loaded or has a fragment file`.

Cause: `nerdctl compose up` can succeed without starting persisted
containers. nerdctl health-check systemd timers are named after the full
container id (`{id}.timer` / `{id}.service`); leftover transient units
produce that warning. The warning is a nerdctl defect, not proof that
Effigy owns those units.

Fix: re-run `effigy container up`. Effigy starts only currently declared
Exited/Created services (Compose project label match), skips one-off
`compose run` containers and undeclared orphans, keeps volumes, and does
not delete systemd units. Before each start it inspects that one container
and starts it only when the observation proves it is stopped; a running or
in-flight container is left to the readiness wait, so a container Compose
already started is never started again. When a stopped owned Colima
container hits that collision, Effigy inspects the full hexadecimal ID and
recovers only an exact-owned transient unit pair in the selected profile
(`systemctl stop` of `{id}.timer`, then `reset-failed` of
`{id}.service`/`{id}.timer`). After that stop/reset it verifies with short
bounded re-probes that both units actually unloaded; a
stopped-but-still-loaded pair fails with its load/fragment state rather
than claiming recovery. A start that reports the collision while the
container ends up running is recorded as a warning and the service goes on
to the normal readiness and health wait, which proves or fails its health
check; the warning names the exact inspect command. A service that stays
stopped gets one retry after that exact-owned unit recovery. Effigy does
not recover running, foreign, one-off, undeclared, persistent, or
unverified units, and it does not change Docker start. Inspect and start
are bounded; a hung nerdctl command still names
the last observed status. Start exit 0 is not readiness: if the service
stays stopped, the error keeps the start backend text (including a
stale-timer warning or the refusal diagnostic) and names:

```bash
colima nerdctl --profile <profile> -- start <container>
```

When automatic recovery is refused, that error also names the inspected
id/status/project/service and the exact `colima ssh --profile <profile>`
`systemctl show` commands for `{id}.timer` and `{id}.service`.

`effigy container status` shows the live `Exited` row rather than an empty
or assumed-running service list.

Under host saturation Effigy distinguishes the machine from the service: an
owned inspect that times out is retried with the same bound (at most twice)
and a saturated host opens the first exec-readiness window from two to fifteen
seconds once before any restart, so a healthy service the host never let
answer is not restarted. When a bounded call still cannot answer, the error
names the observed host load (`load average 40.00 over 18 CPU(s)`) and says the
service's state is unproven instead of blaming it. `Exited (141)` is reported
as SIGPIPE — a closed stdout/stderr pipe — not as a health verdict; read the
service log for the writer that closed. Detached `container up` writes backend
output to one retained file, so Effigy holds no pipe a service could lose, and
the original nerdctl reader was never identified. Let the host quiesce and retry
once rather than widening timeouts or recreating the stack.

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

Effigy reports and repairs it:

```bash
effigy doctor                            # /run inode use and pid-file counts per running profile
effigy container profile prune-health-pid-files --profile effigy
```

`effigy doctor` prints an evidence line for every running profile
(`colima-profile `effigy` /run: 270087/819200 inodes used (32%), 269082
health-check pid file(s) (268182 older than 10m)`) and warns once inode use
reaches 80% or 10,000 stale files accumulate. `container up` and a task that
starts an environment run the same bounded prune automatically and say nothing
about it in a normal run: it is Effigy's
own housekeeping in a VM tmpfs, and Colima 0.10.3 ships nerdctl 2.3.1, so a
routine nerdctl upgrade note is advice the reader cannot act on. An activation
prints the doctor warning only when `/run` inode use is at the doctor
threshold, and names the prune failure if the prune itself could not run. The
counts and the upgrade note stay in `effigy doctor` and in the explicit prune
command. No command that does not touch containers runs the prune. The prune removes only files whose name matches
exactly `health-check-*.pid`, that sit directly inside a containerd runtime-v2
task directory, and that are older than ten minutes; a younger file may belong
to a check still in flight, so it is kept. Nothing else under `/run` is read or
removed. The command reports the counts and `/run` inode use before and after,
and `--json` emits `effigy.container.profile-prune-health-pid-files.v1`.

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
- restarts it when a required managed setting drifted, for example a profile
  started outside Effigy with `forwardAgent: false`, and says which setting it
  repaired;
- names what the deletion lost after it recreates the profile: that profile's
  containers, images and volumes are gone, and Effigy rebuilds the containers
  the run needs.

Creation and restart never delete the profile itself. `effigy container profile
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

## Port registry capacity and recovery

Generated container ports come from the gateway endpoint Store, which imports
`~/.effigy/ports.json` automatically and retains one 100-port range per project. Exhaustion blocks new allocations until a range is
released. Check capacity first:

```bash
effigy container ports status --json
effigy container ports reclaim --json                 # dry run, writes nothing
effigy container ports reclaim --apply --expect-plan <DIGEST> --json
```

`status` and the dry run are read-only. They take no registry lock and create
no files. Each row is `retain`, `hold` or `reclaim` with a reason, and the
report includes free range capacity before and after the planned reclaim. The
plan digest is over the decisions. `--apply` recomputes the plan under the
gateway Store gate and refuses to write if the digest differs from the reviewed one.

When registry capacity is at or near exhaustion (zero or one free 100-port
range), ordinary `effigy container status` output ends with a one-line text
note pointing to `effigy container ports status`. The note is discoverability
only: it reads the registry without the lock and computes capacity, performs
no allocation, recovery planning or probes, and leaves the status JSON schema
unchanged.

An entry is reclaimed only when all of these hold:

- its project path is definitely missing: the leaf is absent and the nearest
  existing ancestor is a readable directory, so a missing parent under a
  readable root proves absence just as well as a missing leaf. A real I/O or
  permission error, a non-directory ancestor, and an absent volume root such
  as an unmounted `/Volumes/...` are unknown and hold the entry;
- no runtime-scope record that still retains that project's volumes or routes
  names the project or its checkout. A record whose profile is observable and
  that retains nothing is decided by that profile's own inventory instead:
  no container there means reclaimable;
- no gateway route names the project or path;
- no container in any state (running, Created, exited) carries the project
  label or working directory, and none publishes a host port in the range;
- no host port in the range is in use on this machine (a wildcard bind probe
  fails, or cannot be attempted);
- no other allocation records the same project path;
- no stopped Colima profile can own it. A stopped profile publishes no host
  port, so it never has to be started to answer the bound-port question, and
  it holds only the entries it is recorded on: those whose runtime-scope
  record names that profile. An entry with no recorded profile is attributed
  to the configured Effigy profile. When that profile itself is stopped, every
  entry attributed to it is held rather than reclaimed.

Present paths are always retained.

Incomplete proof holds the whole registry. An unreadable scope store or route
table, or a container inventory that failed for a running runtime, holds every
absent-path entry. A stopped Colima profile is not one of those: it is reported
per profile and scoped to the entries that profile can own, with a reason on
each held entry.

Under capacity pressure — fewer than ten free 100-port ranges — a `container
up` that needs a new range first reclaims provably dead entries by exactly the
rules above, then allocates, and prints one line saying how many ranges it
reclaimed and how many remain. It reads the same evidence, backs the registry
up first and recomputes the plan under the registry lock. Reading capacity and
planning a reclaim never apply it, so `status` and a dry-run stay read-only.

Apply removes only the reclaim rows. It saves the pre-edit endpoint document as
`ports.json.reclaim-backup-<nanos>` and reports both document SHA-256 values.
The imported original stays unchanged. Released ranges remain quarantined for
at least 70 seconds before they can be allocated to another owner. It never
touches containers, volumes, data, routes, certificates, profiles or loopback
assignments. Normal `container down` keeps stable allocations. `container
retire` releases the retired scope's own project allocations through its
existing record path. Loopback IPs and dormant domain ownership also survive
ordinary down/up; retirement explicitly releases the recorded owner's claims.

Limits: the bound-port probe is a point-in-time check. A backup left by a
crash between backup and rename is not removed automatically. Recovery does
not expire entries by age.

## Dead loopback owners

Each isolated project and shared service holds one address in the bounded
`127.1.0.1`-`127.1.0.50` loopback pool, and its container routes keep that
address reserved. A project whose checkout, containers, runtime scope and
listeners are all gone would otherwise hold its address forever, and a new
stack then fails with `loopback pool exhausted`. Preview what Effigy would
release, then apply it:

```sh
effigy gateway repair --json                       # preview, writes nothing
effigy gateway repair --yes --expect-plan <DIGEST> --json
```

The preview is read-only. It lists every assignment and container route with
its decision (`reclaim`, `hold` or `retain`) and a reason, plus a plan digest
over those decisions. `--yes` releases the entries its locked recomputation
still proves dead; `--yes --expect-plan <DIGEST>` refuses to write when that
recomputed plan differs from the digest you reviewed. `--yes` alone applies
the plan it recomputes and prints the digest it applied.

A loopback assignment or container route is dead only when all of these hold:

- its owner checkout path is definitely missing: the leaf is absent and its
  parent directory is readable, the same classification `container ports
  reclaim` uses. Permission and I/O errors, and an absent parent such as an
  unmounted volume or a vanished intermediate directory, are unknown and hold
  the entry;
- no runtime-scope record names the identity, its checkout path or its route;
- no container in any state carries its project label or working directory,
  and none publishes on its address;
- no live assignment or live project route keeps the address;
- no non-gateway process listens on the route's alias tuple. The gateway
  binds its aliases from the Store's route table and stops a listener when its
  route disappears, so its own alias for a dead route is not a hold; a
  same-user process that is not the recorded gateway, or a probe that cannot
  answer, is.

Allocation, its automatic collection and this preview prove ownership from the
same inventory — every container state on every profile, with a stopped profile
recorded as stopped — so the automatic path can never release what the preview
would hold.

A shared-service identity is dead only when no consumer project that uses it
is live and no container carries the shared service's own project name. Incomplete ownership proof holds the whole plan and names the reason;
a stopped Colima profile does not hold anything, because it binds no host
address and cannot start a stack whose checkout is gone. Present paths, running
containers and unknown path states are never touched, so a live project's
address never changes.

Allocation also self-heals: when `container up`, `dev` or gateway registration
would fail because the pool is full, Effigy releases the dead entries once
under the same gate, prints one line naming how many assignments and routes it
released, and retries once. The endpoint Store keeps a released address out of
reach for one reuse window (about three minutes: DNS TTL plus drain and the
store's operation budget) so a cached answer never lands on a new owner, so an
allocation that needed the release succeeds once that window has passed.

Apply writes under the endpoint Store's owner gate and keeps a pre-edit copy of
both endpoint documents as
`~/.effigy/gateway/loopback-dead-reclaim-backup-<nanos>.json`, reporting both
SHA-256 values. It never touches containers, volumes, data, profiles,
certificates or live routes. Growing the pool past `127.1.0.50` is not part of
this surface; the `lo0` aliases come from `gateway setup`.

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
