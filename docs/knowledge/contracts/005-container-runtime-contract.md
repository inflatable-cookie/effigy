# 005 - Container Runtime Contract

Owner: Platform
Last Updated: 2026-10-04

This contract defines the required runtime guarantees for container-backed
task execution in Effigy.

It covers the execution surfaces that depend on a running local container
environment, not production deployment export.

## Purpose

Effigy now has several public surfaces that rely on the same local runtime:

- managed `dev` tasks
- standard routed tasks with `run_in = "container"`
- workspace shell handoff
- bootstrap flows that dispatch container-backed tasks

Those surfaces must not drift in semantics based on whether the command was
launched through a TUI, a plain shell, or a one-shot bootstrap path.

This contract exists to keep one explicit runtime guarantee across those
surfaces.

## Runtime-backed execution surfaces

The contract applies to any Effigy surface that:

- resolves a task or shell into a container-backed workspace
- auto-starts or reuses a local runtime
- dispatches work through container exec or container-local Effigy handoff

The first covered surfaces are:

- managed `dev` flows
- standard routed task execution
- `effigy workspace`
- bootstrap-driven container task execution
- Rhai container-targeted execution helpers

Direct operator use of raw compose commands is outside this contract.

## Linked worktree ownership

Effigy gives each linked Git worktree generation a stable runtime token stored
in Git's private worktree directory, outside the checkout. Generated Compose
project names include that token even when a manifest supplies `project_name`.
The project name scopes Compose containers, networks and managed volumes;
generated published ports use a project-specific allocation, including when
the manifest names a fixed host port. Repeated commands in one worktree reuse
the token. Recreating a worktree at the same path creates a new token. The
primary checkout keeps its existing project name and port behavior.

`effigy container scope --json` is the policy-independent checkout identity
query for archive callers. It resolves the selected checkout before deletion
without loading a container declaration, starting a backend, or inspecting
containers. Its versioned result reports the absolute checkout path, scope
kind (`worktree`, `ephemeral-clone`, or `none`), and the full token or `null`.
The exact success payload is:

```json
{
  "schema": "effigy.container.scope.v1",
  "schema_version": 1,
  "ok": true,
  "checkout": "/absolute/path/to/checkout",
  "scope": {
    "kind": "worktree",
    "token": "0123456789abcdef0123456789abcdef"
  }
}
```

For an unscoped checkout, `scope.kind` is `none` and `scope.token` is JSON
`null`. The token is always the full 32-character generation identity, never
the shortened host key.
For a scoped checkout with no token yet, the query creates that generation's
token in its private Git directory; it does not create runtime resources.
Primary checkouts and unmarked clones return `none` and `null`. Invalid Git
metadata is an error, not an unscoped result. Queue persists the token before
archive and uses the same value with `container retire --scope <token> --yes`
afterward. `container hosts --json` also exposes a scope token when a container
policy loads, but it is a host-map query rather than this general identity
contract.

`share_runtime_identity = true` is an explicit opt-in to one Compose identity
across worktrees. It must not arise merely from equal `project_name` values.
Repo-owned Compose files keep their own resource rules: linked worktrees fail
closed when they contain fixed published ports, container names, or named
external resources that Effigy cannot scope.

Gateway route ownership is the checkout path plus its generation token. Route
table claims and removals serialize under a lock separate from the atomically
replaced table file. A live foreign owner blocks a domain claim, including
HTTP, TLS and TCP alias routes. Teardown and stale-route pruning remove only
routes still owned by the caller; certificate deletion follows the same
owner-checked removal while the lock is held. A missing or replaced worktree
generation is stale and may be claimed by another worktree. A fixed domain
therefore refuses a second live worktree until the consumer supplies distinct
domains. The route table never silently moves a live sibling's domain.

A linked worktree that does not share runtime identity gets one effective host
map for its declared HTTP routes and TCP aliases. The apex becomes
`<apex>-w<host-key>.<tld>` (for `app.test`, `app-w<host-key>.test`); helper
hosts such as `mail.app.test` stay under that apex. The primary checkout keeps
declared names. `effigy container hosts` returns declared and effective names,
origins, cookie domain and WebAuthn relying-party id. Effigy registers those
effective names on the gateway. It does not rewrite application configuration.

Owned mutable resources for that generation carry `com.effigy.scope`,
`com.effigy.project`, or the exact Compose project label. `effigy container
retire` deletes only resources those proofs still attribute to the recorded
token, and only on the runtime profile that produced the labelled
observation. Listing uses `ps -a`, so Created and exited containers retire
with running ones. Disposable scoped build-cache volumes, including legacy
`efv-*` caches that still carry an ownership label, are removed; persistent
application data is not. Same-named resources in another profile stay.
Shared services, persistent and external volumes, and foreign-owned
resources stay. Shared-identity routes and loopbacks stay even when the
same worktree also has isolated stacks. Compose networks are part of the
owned inventory. A durable record under `~/.effigy/runtime-scopes/` survives
checkout deletion so cleanup can retry; one record per generation token
aggregates every container environment and every runtime profile in that
worktree. Record writes take a lock and replace the file atomically; a
corrupt record is an error, not an empty scope. Backend discovery failures,
including malformed inspect or listing JSON, keep the record and do not
report success. A recorded profile whose runtime is stopped is probed first,
never skipped silently and never started: retire still clears host-side
resources (routes, ports, loopbacks, certificates) and live profiles, then
exits non-zero, lists the profile under `unverified_profiles`, and keeps the
record. A stopped VM is not proof that its persisted containers or volumes are
gone; start the profile and rerun retire. Success requires that no owned container, mutable volume,
network, isolated route, port, loopback, or TLS certificate remains. A
second retire with nothing left is success, including a shared-only scope
whose durable record is then removed.

Resolving, discovering, planning, or reporting container policy does not
register runtime ownership. Runtime activation records the scope before
starting the backend or creating containers, networks, shared services,
routes, or ports. The registration uses the checkout generation token and
preserves the isolated or shared-identity policy; a failed start leaves its
record available for retry. Checkout-based retirement consults existing
records only. A configured container with no activation record is not proof
that its profile was activated, so retirement reports no owned scope without
starting or probing that profile. Existing records, including records naming
stopped or unavailable profiles, remain authoritative and fail closed when
their resources cannot be observed.

Tom’s 2026-10-01 recovery ruling, "do what you need to for 061", allowed one
exact-scope exception: start the stopped profile to observe its resources,
retire only resources proven owned by the approved scope, and retry only the
recorded milestone teardown while preserving its QA result. Restore the
initial profile state only if no other active owner appeared. This does not
permit automatic profile starts, treating stopped resources as absent, broader
host cleanup, or scheduler activation.

Loopback allocation and reclamation must follow the same ownership boundary.
An isolated project's loopback key is `project:<project-name>:<absolute-checkout>`
in both generated Compose preparation and gateway registration. A legacy bare
project key may move to that qualified key only when its recorded scope equals
the checkout and its address is not already assigned elsewhere. Retirement
clears qualified keys and legacy bare keys only when the recorded scope proves
the same checkout owns them. Shared loopback identities remain retained.

Tom’s 2026-09-30 ruling: Colima-only use must not require starting Docker
Desktop to permit safe loopback reclamation. Runtime discovery must distinguish
a proven inactive optional backend from a failed authoritative inventory.
An executable on PATH or a missing socket alone does not prove ownership.
The repair must preserve uncertain owners; it does not authorize live registry
cleanup or pool expansion. Uncertain evidence includes symbolic or dangling
Colima instance entries and explicit Docker environment overrides that cannot
be decoded. Neither may be treated as an absent runtime; failed discovery
must keep their owners protected.

Stale reclamation requires a complete inventory across every participating
runtime: Docker's default backend when Docker participates, and every running
Colima profile. A successful empty inventory proves that no running owners
were found and may release stale isolated assignments. A failed profile
listing, runtime `ps`, or row parse makes the inventory incomplete;
reclamation then preserves all uncertain assignments and reports the backend,
profile, error, and skipped-prune reason. The allocator may use unassigned
capacity, but a full pool remains a capacity error that includes the discovery
failure. Increasing the pool cannot substitute for identity and discovery
correctness.

Participation is decided before any `ps` discovery, without launching Docker,
Colima, or a container. A proven-inactive optional runtime is skipped rather
than reported as a failed inventory. Docker considers every endpoint that could
be effective — `DOCKER_CONTEXT`, then `DOCKER_HOST`, then the stored current
context, then the platform default Unix sockets — so a remote context selected
by `DOCKER_CONTEXT` is never omitted when `DOCKER_HOST` names an absent local
socket. The `DOCKER_CONFIG` path and context names are read exactly as given,
so normalization cannot hide a selected context. Docker is inactive only when
every candidate is an absent local Unix
endpoint, including a dangling `/var/run/docker.sock` symlink. A reachable local
endpoint, any remote or unresolvable configured endpoint (`tcp://`, `ssh://`,
`npipe://`, a missing context, unreadable or schema-invalid config, or
`DOCKER_HOST`), and a socket that exists but cannot be connected are all
ambiguous and stay authoritative, so a later failure remains actionable and
fail-closed. Colima is
inactive only when its CLI is absent, its Colima home can be inspected, and no
Colima or Lima instance state remains, because a running Colima VM can outlive
its client CLI. Docker context names are case-sensitive, so a context named
`DEFAULT` is selectable and is not collapsed into the default context. Neither
the preferred backend, CLI presence, CLI absence alongside remaining or
uninspectable state, stderr text, nor an absent socket on its own proves that a
runtime owns nothing. Identical skipped-reclamation warnings are emitted once
per process; distinct failures still print.

Workspace archive must invoke `effigy container retire --yes` while the
checkout still exists. After the checkout is gone, retry with
`effigy container retire --scope <token> --yes`. Failure is non-zero and
leaves the durable record. `bootstrap teardown` is only for `--fresh`
bootstrap sessions, not worker scopes. The intended Paseo hook is
`paseo.json` `worktree.teardown`; Queue `workspace.archive` does not call
retire itself. A consumer that still calls `bootstrap teardown`, checks
only running containers, or returns 0 on residue needs its own change.
This repository's own `paseo.json` hooks call `scripts/worktree-lifecycle.sh`
instead of an installed `effigy`. `setup` discards any staged binary, builds
`effigy` from the checkout with `CARGO_TARGET_DIR` pinned to
`target/worktree-hooks`, stages it at `.local-install/worktree-hooks/effigy`,
then runs the skill `prepare ../effigy-catalog-pack` and `link`. `teardown`
runs `container retire --yes` and then skill `unlink` with the staged binary,
so Queue milestone cleanup exercises the lifecycle code under test. Setup
stages the binary even if later QA fails. A missing or failed build, prepare or
retire exits non-zero; teardown never falls back to the PATH `effigy` and does
not unlink after a retire failure. Limits: the helper compiles with raw
`cargo` as bootstrap, does not install anything globally, and a checkout whose
setup never ran cannot be torn down by this hook. Covered by
`test:worktree:lifecycle-hooks`. `NORTHSTAR_SKILL_PATH` still overrides the
skill location.

Repo-owned Compose is classified as such: cleanup uses the scoped project
label, not a name prefix, and named volumes in those projects stay unless
labelled `com.effigy.persist=false`. `share_runtime_identity = true` skips
stack deletion. Certificate removal for retired TLS routes is recorded on
the scope first, then runs as one owner-checked operation under the
route-table lock before the table is saved. A TLS failure leaves the route
and the pending-cert list for retry. A pending certificate whose domain now
has a foreign owner is left in place.

## Approved ephemeral clone extension

Tom approved the same runtime scope for a full clone whose local Git config
sets `effigy.runtimeScope = ephemeral`. The orchestrator sets this marker when
it creates the clone. Effigy stores that clone's token in its own
`.git/effigy-runtime-scope`; deleting and recreating the clone creates a new
generation. `is_live`, effective hosts, route ownership and `container retire`
must use the same scope behavior as linked worktrees. An unmarked full clone
keeps primary-checkout behavior. A marked clone has its own Git directory and
needs no linked-worktree shared-Git-directory mount in the container.

The marker read is local-only: Effigy parses the checkout's own `.git/config`
without a `git` subprocess and without following `[include]`/`[includeIf]`
directives, consulting `--system`/`--global` scope, or reading `GIT_CONFIG_*`
environment. Only a writer that puts the marker inside the checkout's own Git
directory can scope it, so inherited or global configuration can never mark
an unmarked checkout. Section and key names compare case-insensitively as
Git does; the value must be exactly `ephemeral`.

Generated Compose project names carry the token behind a scope-shape tag:
`-wt-` for linked worktrees, `-ec-` for marked ephemeral clones. `effigy
container hosts` reports scope kind `ephemeral-clone` for a marked clone and
`worktree` for a linked worktree; host rewriting is identical for both.

## Contract goals

When Effigy routes work into a container-backed runtime, it should guarantee:

- one clear runtime-prep phase before exec or handoff
- one stable container-handoff marker contract
- one honest alias-resolution contract inside the execution target
- one explicit fallback boundary when the compose backend does not provide the
  expected runtime behavior directly

## Source of truth

The runtime contract derives from:

- the captured `EffigyRuntimeContext`
- the effective manifest
- the resolved container policy
- the generated compose/runtime metadata
- the live runtime state when readiness or published-port inspection is needed

It must not depend on whether a user happened to start the runtime previously
through `dev`, `workspace`, `container up`, or `bootstrap`.

It must also not depend on caller-local cwd, env, Docker, Colima, or nerdctl
probing in runner command modules.

## Runtime prep contract

Before Effigy dispatches work into a container-backed target, it must treat
runtime preparation as a required phase, not an optional side effect.

The runtime-prep phase owns:

- ensuring the selected runtime exists and is running
- ensuring required shared services are available
- ensuring the target exec surface is ready for the requested working
  directory and command style
- reconciling runtime guarantees that the compose backend failed to materialize
  directly
- reconciling gateway/runtime exposure needed by the selected execution target
- refreshing non-shell task leases when Effigy owns warm-runtime reuse
- for scoped worktrees, proving a fresh host checkout marker is readable at
  the container working directory and a container write reaches the selected
  checkout before dispatch or lease refresh
- refreshing changed Cargo input mtimes inside that mounted checkout before a
  container-routed check, so host edits invalidate stale container artifacts

Surface-specific presentation may differ. The prep contract must not.

Runtime prep must consume captured context facts and typed execution policy. It
must not rediscover invocation cwd or handoff state after request construction.

## Cold container launch readiness

For Colima-backed `effigy container up`, Compose must not start until the
runtime is usable. A running VM is not enough: both a profile-scoped
`nerdctl info` and `buildctl debug workers` inside that profile must succeed.
A warm profile returns after those probes. If a running profile stays
unavailable through the bounded grace period, Effigy repairs that profile once
and probes again; a newly started profile also waits for both probes. Failures
identify the runtime stage and include the last probe result.

After Compose up, Effigy waits before registering gateway routes until the
runtime reports the matching project and declared service publishing each
selected port. The same wait covers declared TCP service aliases. Stopped
runtime rows are inspected on timeout so an exited service is named with its
exit status. The existing project, service, published-port and host-listener
ownership checks still gate every route claim. No route is claimed from a
fallback port while the declared runtime binding is absent.

## Owned service start after Colima restart

A running Colima VM can still leave Effigy-owned Compose containers `Exited`
or `Created` after the VM restarts. `nerdctl compose up` may return success
in that state; `nerdctl start` of the same owned container is the recovery
that preserves volumes, including Postgres crash-recovery data.

After Compose up, Effigy inspects owned projects including stopped
containers. A container is a recovery candidate when its Compose project
label matches the selected environment or a declared shared-service
project, its compose service is currently declared in that project's
compose files, and it is not a one-off `compose run` container
(`com.docker.compose.oneoff=True`, or a `-{service}-run-` name).
Undeclared orphans are not started. Matching stopped containers are
started by name. Effigy does not recreate them, does not delete volumes,
and does not delete systemd units, including nerdctl health-check timers
named after container ids.

Start exit 0 is not readiness. A nerdctl warning that
`Unit <id>.timer was already loaded or has a fragment file` is recorded when
inspect then shows the owned container running; systemd units are not
deleted. A start that reports that collision while inspect shows the
container running is recorded as an unresolved health check with the exact
inspect/start commands rather than as a clean recovery. If the owned
Colima/nerdctl container stays stopped with that collision, Effigy may
recover once: inspect the container for a full 64-character hexadecimal ID,
require stopped status and the selected project/service labels (not one-off,
orphan, foreign, running, or undeclared), confirm `{full_id}.timer` and
`{full_id}.service` in the selected profile are transient or already gone,
then `systemctl stop` only `{full_id}.timer` and `systemctl reset-failed`
`{full_id}.service` and `{full_id}.timer`. Because operation exit 0 is not
proof that a stopped transient unit unloaded, Effigy then re-probes the
exact pair with a bounded wait until both units are unloaded; a late unload
recovers once, and a pair that never unloads fails with its still-loaded
load/fragment state and a bounded diagnostic instead of claiming recovery.
A persistent or otherwise unverified unit appearing after the stop/reset is
refused without deleting files. The single start retry then proves readiness
by inspect.
Stop/reset of already-gone units is idempotent. Persistent fragments,
mismatched unit identity, unknown status, missing inspect ID, unavailable
systemctl/Colima authority, or Docker backends are not recovered
automatically; the failure names the inspected identity, the refusal
reason, and the exact profile-scoped operator commands. Unit files are
never deleted, daemon-reloaded, or selected by wildcard. If the owned
container stays stopped, or inspect/start times out, Effigy fails and
names the service, its last observed status, the start backend text
(including that stale-timer warning, a refusal, or a hang timeout), and
the exact `colima nerdctl --profile <profile> -- start <container>`
command (or `docker start <container>` on Docker). Inspect and start are
bounded so a hung nerdctl command cannot present a partial stack as ready.
`container status` lists those stopped owned rows instead of omitting them
or reporting success by assumption.

Runtime activation planning belongs to `effigy-runtime-plan`. The runner
runtime-prep modules are side-effect adapters for that plan: they may start
the runtime, perform readiness checks, reconcile aliases/routes, and refresh
leases, but they must not invent a separate activation model.

## Handoff contract

Effigy has two valid execution modes inside a running container:

- container-local Effigy handoff
- raw container exec

The decision may depend on container capabilities, but the recursion guard
must be one shared contract.

The runtime handoff marker is:

- env var: `EFFIGY_INTERNAL_CONTAINER_HANDOFF=1`

Meaning:

- when present inside the container, Effigy is already executing inside a
  container handoff
- routing must not recurse back into container dispatch
- `stay_in_shell` and related handoff-only behavior must treat that state
  consistently across managed and standard surfaces

The marker name and meaning are product contract, not incidental plumbing.
`EffigyRuntimeContext` captures marker presence at process entry; downstream
runtime code should consume that captured state when a context is available.

## Container manager contract

Runner-facing container operations must route through `ContainerManager`.

This covers:

- backend selection
- compose invocation shape
- container exec and shell operation shape
- copy, logs, status, stats, up, and down operation shape
- backend-owned repair or retry behavior
- attached-session interrupt closeout
- internal operation reports

Backend-specific Docker Compose and Colima/nerdctl behavior belongs behind the
manager facade. Runner command modules may request a container operation, but
must not branch on backend internals or construct `docker`, `colima`, or
`nerdctl` process commands locally.

The detailed manager contract lives in
`012-container-manager-contract.md`. The cross-pipeline runner boundary lives
in `015-runtime-operation-pipeline-contract.md`.

## Activation ownership

Effigy has two valid ownership models for container-backed local work:

- public shell/session ownership
- non-shell task activation ownership

These must not drift by caller path.

### Public shell/session ownership

This covers:

- `effigy dev`
- `effigy workspace`
- `stay_in_shell` handoff flows

Contract:

- Effigy prepares the runtime for interactive access
- gateway/public route exposure must be reconciled before the shell opens
- session shutdown ownership depends on whether Effigy completed runtime
  readiness for that shell

This is not lease-managed task activation.

### Non-shell task activation ownership

This covers:

- standard routed tasks with `run_in = "container"`
- deferred requests with `[defer].run_in = "container"`
- bootstrap `run` steps that dispatch into a container without opening a shell
- Rhai `exec::run(...)` requests with container runtime policy

Contract:

- the runtime-prep phase runs before user command dispatch
- if Effigy auto-started the runtime, or the runtime was already under an
  active task lease, Effigy refreshes the host-container lease
- default lease timeout is 5 minutes unless configured otherwise
- lease reuse must not depend on whether the request came from deferral or
  explicit task routing

This is the required shared contract for warm non-shell container reuse.
Task-shaped requests should reach this contract through
`TaskExecutionRequestBuilder` and a resolved execution plan.

## Workspace identity and disposable build paths

When a container declares `workspace_user`, these entry paths must prepare
declared disposable Rust build/cache paths for the resolved numeric uid/gid
before children launch:

- headless workspace / public session handoff
- primary-service `effigy exec`
- routed and deferred container tasks

Plain primary-service `effigy exec` requires that the primary service is
already running. If it is down, Effigy reports `effigy container up <NAME>`
and returns before workspace repair or the requested child. `effigy dev` is
also a project start path. Plain exec does not start the VM, service, or
gateway. Explicit `container up` and the existing routed-task activation path
continue to own provisioning.

Preparation classifies each declared mount from current policy and compose
source, not from the host login name:

- Effigy-owned named volumes exclusive to the primary service, and
  image-layer home caches: repair unowned nested contents, then verify
  actual read/write/create-lock as the resolved non-root user. Repair is one
  bounded `find -P -xdev ! -type l ... -execdir chown -h` exec per volume
  (runtime round trips are O(volumes), not O(files); native traversal still
  scales with entries). Exec preparation batches plan metadata and numeric-user
  access checks; one bounded ownership scan checks all owned paths and launches
  the bulk repair only for dirty volumes. Clean paths therefore launch no
  repair. Effigy rechecks on every exec because nested permissions can change
  through external writers and the current runtime provides no exact cache
  invalidation signal; no cached pass skips those checks. `-execdir` runs
  `chown` on `./name` from a directory fd held by `find`, so an intermediate
  directory swapped for a symlink mid-run cannot redirect ownership changes
  outside the volume. It needs GNU findutils in the image; without it repair
  fails not-ready. The child is bounded by a 600 s cap (or the caller deadline
  if sooner) and reaped on expiry. The batched ownership scan has the same cap.
  Recursive scans prune every declared child mount path from the current
  compose ownership plan, including bind and nested-volume targets. `-xdev`
  remains an additional guard; path pruning also protects mounts that share a
  filesystem device. A nested owned volume is scanned in its own scope after
  the parent scan skips it. A nested host bind is checked for read/write access
  as the resolved numeric workspace user, without comparing its observed owner
  to that identity and without creating a probe file in the bind. Unwritable
  binds fail with the mount source and numeric-user repair guidance; Effigy
  never repairs their root or contents. Scans never follow symlinks or leave
  the current repair scope. Readiness requires a post-repair unowned listing,
  a scope identity re-check and the access probe, not chown exit alone.
  Private acceptance (disposable container from a
  local GNU-find image, no host mounts or network, `--ignored` case in
  `test:workspace:rust-ownership:bulk`): 44339 entries over three volumes were
  repaired to 501:20 in 3 runtime execs and 193 batched native `chown`
  invocations (about 2.2 s real; the per-file model would be 88678 execs), then
  to 1000:1000, with content manifests identical, numeric-user
  read/write/create verified, idempotent reruns doing no chown, a deep failing
  path returning not-ready after partial progress, and the directory-swap race
  leaving outside files untouched (the old `-exec` form escapes). This is a
  private fixture, not installed-consumer acceptance
- bind-mounted rust `target` or cargo paths: verify only; never chown host
  source, siblings, or shared caches
- named volumes used by two or more compose or managed services: rust
  caches verify only; other shared named volumes are forbidden
- read-only, external, or foreign mounts: refuse mutation; fail closed when
  they are declared rust caches

A matching owner at the mount root is not enough. Nested unwritable
`target/debug/.cargo-build-lock` and Cargo `registry/src` / `git/checkouts`
must be repaired when the path is owned-disposable, or reported with an
exact safe command (runtime profile, container, path, numeric identity)
when it is not. Failures are not swallowed and must not claim ready.
Symlinks and path escapes are not followed. World-writable `chmod 777` and
recursive host-source chown are forbidden. Repair may restore owner write
(`u+w` / `u+wx`) on owned disposable paths after a failed write probe.

Tom's 2026-10-05 ruling: “host-mounted ownership is expected, do not
restructure the container to appease ownership.” Preserve intended host bind
layout; do not move a shared cache to a named volume or disable its mount to
avoid ownership preparation. Root-observed ownership differences are
acceptable when numeric-user access works.

`effigy doctor` remains read-only. Finding id `container.workspace-ownership`
covers declared cargo/target mounts, their nested rust lock/cache paths, and
managed disposable paths. It batches read-only metadata for known mount roots
and each required nested path depth, then checks read/write access as the
resolved numeric uid/gid for present non-symlink paths. Overlapping declared
targets reuse a sampled nested path. A sample on a mount stops its later
nested probes; the doctor does not walk the whole tree. Every preliminary
liveness probe (Colima profile `status`, the primary-service Colima/`compose
ps` check, and the SSH-agent socket preflight) and every metadata or numeric
user batch converts the same remaining doctor deadline exactly once,
immediately before spawning, so no subprocess gets a fresh budget of its own.
An already-expired deadline never spawns a probe, and a hung probe is killed
and reaped at that deadline (only its own recorded process group). An
unavailable batch discards partial permission samples and reports verification
incomplete with its sampled paths; it never reports a permission fault or
clean ownership. A bounded doctor run never triggers a Colima runtime repair.
Stopped, unavailable, and no-workspace-user states are distinct from clean.

## Alias contract

Effigy owns two related but distinct alias surfaces:

- host-visible local domains and service names
- container-local service resolution needed by container-backed tasks

Those must not be conflated.

### Host-visible alias contract

Effigy gateway and runtime orchestration own the host-visible `.test` naming
surface.

That includes:

- HTTP route domains
- TCP service aliases derived from shipped service catalogs
- project-owned aliases
- shared-service aliases where several project-facing names collapse onto one
  shared backing-service identity

This surface is validated against live runtime port data where needed.

### Container-local alias contract

Container-backed tasks must be able to resolve the service aliases they
depend on from inside the container execution target.

The first guaranteed alias class is:

- TCP backing-service aliases such as `mysql.<site>.legacy.test`

The guarantee applies inside any container target Effigy uses for:

- workspace handoff
- routed task execution
- container-local Effigy handoff

It does not promise that every arbitrary compose service in the runtime sees
those aliases automatically. The guarantee is scoped to Effigy-owned execution
targets.

### Alias source rules

Container-local aliases must derive from the same effective service-alias
model as the host-visible TCP alias surface.

That means:

- project-owned aliases keep their declared domain
- shared-service aliases may resolve through one shared backing-service host
- explicit route/alias precedence must remain stable

Effigy must not invent a second alias naming scheme for container-local
resolution.

## Fallback ownership

Compose backends do not all materialize runtime features equally.

When a supported backend fails to provide the required alias behavior or other
covered runtime guarantees directly, Effigy may repair that gap during the
runtime-prep phase.

That fallback is legitimate product behavior when:

- the repaired state preserves the documented runtime contract
- the repair derives from the same effective model as the non-fallback path
- the repair is scoped to Effigy-owned execution targets

The current expected fallback class is:

- container-local TCP alias reconciliation when the backend does not expose
  service aliases reliably inside the execution target

Fallback ownership must stay explicit in code and docs. It is not acceptable
for one surface to rely on backend luck while another surface performs the
repair.

The same rule applies to task activation:

- one non-shell caller path must not refresh leases while another caller path
  tears the runtime down immediately
- one caller path must not reconcile gateway/public routes while another
  caller path silently skips them for the same container policy

## Failure semantics

If runtime preparation cannot establish the contract required for the selected
execution target, Effigy should fail before dispatching the user command.

Examples:

- target service cannot satisfy exec readiness
- required alias reconciliation cannot resolve its backing service
- runtime policy and live runtime state disagree in a way Effigy cannot repair

Failure should report the runtime guarantee that could not be established, not
just the downstream task failure it would have caused.

A scoped worktree whose running primary service reads a different checkout
fails before task execution. Effigy removes the temporary probe on both success
and failure. A sibling's live runtime and persistent data are left alone.

## Validation direction

This contract should be covered by targeted compatibility tests rather than
large generic runtime smoke tests.

The minimum proof set should cover:

- managed execution and standard execution reaching the same handoff semantics
- workspace handoff and bootstrap-backed task execution sharing the same alias
  guarantees
- runtime prep repairing backend-sensitive gaps before the user command runs
- owned Exited services after a Colima restart started or diagnosed without
  deleting systemd units
- compatibility behavior on the supported Colima + `nerdctl compose` path
- runner container commands routing through `ContainerManager`
- container-targeted execution plans consuming captured runtime context instead
  of caller-local cwd/env probes
- runtime activation plans preserving repo root, repo override, policy name,
  container identity, and lease policy across `effigy exec`, workspace, and
  managed surfaces

## Drift triggers

Update this contract when Effigy changes:

- the handoff marker name or meaning
- which execution targets receive container-local alias guarantees
- the boundary between gateway-owned and runtime-owned alias behavior
- the runtime-prep steps required before exec or handoff
- owned-service start after a Colima VM restart, including the boundary
  against systemd unit deletion
- the supported backend fallback model
- runner-facing container manager operation ownership
- runtime context facts used by container-backed execution
- runtime activation request/plan/report fields

## Verified local runtime refresh — 2026-10-04

Under Tom’s standing authority `b16abcd4`, the planner refreshed the local host,
ARM64 Linux artifact and canonical skill to `v0.13.1+local.ffdfc1f9`, from
reviewed main `ffdfc1f9b6368461ed87cc745859532d422a6e27`. Queue milestone
`c15c9959-2329-4d4f-ad5e-01728a70acbc` passed setup, QA and teardown, with
no escaped descendants and automatic capacity release. Both private native
candidates and installed host routing checks passed. The Linux build’s
original scheduler run settled passed after its follower lost contact while
queued; the idle-read failure remains separately owned by Effigy #077.

This refresh includes the exact-owned stale healthcheck recovery and Rust-path
ownership diagnostics/provisioning. Automatic repair remains limited to
exclusively owned disposable volumes. Bind-mounted targets and shared caches
are verified only; real consumer UID/mount acceptance is still required and
was not established by private numeric-UID memory fixtures. No live ownership
repair, VM start, stack restart or release was performed. Historical admission
state was preserved. The actual prior host `056111f` and Linux `47078933`
channels, skill files and hashes are backed up in
`~/.cache/effigy/rollbacks/runtime-ffdfc1f9-4cquw2gs/rollout-receipt.json`.

## Release-unblock local refresh — 2026-10-04

Under standing authority `b16abcd4`, the planner refreshed the host, ARM64
Linux artifact and canonical skill to `v0.13.1+local.c6f4f33c`, pinned to
`c6f4f33c0038b1f25cae335a35708a063f16c0d0`. Queue milestone
`132db11b-5876-4099-abae-3aa9c2451735` passed setup, QA and teardown.
Its capture recorded inherited parent-token children, the QA nested fact,
unchanged historical admission state, no escaped descendants or reapers,
and automatic capacity release at settlement.

Host and native ARM64 Linux candidates passed version, light routing,
unavailable-scheduler exit 75 and retired explicit-zero exit 2 checks, with
no heavy-task effects. Whole JSON output parsed successfully on the host.
Installed host checks passed and the previous `ffdfc1f9` binary remained
usable as a rollback artifact. Backups, hashes, private checks and the
install receipt are in
`~/.cache/effigy/rollbacks/runtime-c6f4f33c-e7ycg0c7/rollout-receipt.json`.
A private build-routing mistake stopped the first candidate attempt before
compilation; the corrected attempt completed. No milestone rerun occurred.

This includes #077 idle follower and #079 exact-unit unload verification;
#078 doctor latency and #080 bulk ownership remain separate. Actual fresh
Acowtancy stack acceptance of #073 failed provisioning throughput, and its
log also recorded a deep Cargo checkout chown failure. The owning worker
stopped that bootstrap, preserving services, volumes, logs and worktree.
#080 must pass independent protection/throughput review, a later gated
refresh and real consumer acceptance before that blocker is resolved.
No live ownership cleanup, stack restart, VM start or release was performed
by this refresh. Historical admission state stayed unchanged.

## Legacy nested-bind acceptance and local refresh — 2026-10-05

Tom explicitly approved installation of the prepared `67748464` candidate and
`effigy exec true` in `/Users/tom/Dev/legacy/sites/acowtancy`. The host binary,
ARM64 Linux artifact and canonical installed skill were refreshed from exact
source `677484647f6e0896ba016fc329c921f1251a096f`, after Queue milestone
`1a053222-3931-42af-9e30-ef7abf30b714` passed setup, QA and teardown. Its
settlement recorded no escaped descendants or containers; capacity released
automatically at `2026-10-05T11:54:59.108Z`.

Private host and native Linux checks passed version, light routing, whole JSON
parsing, unavailable-scheduler exit 75 and explicit-zero exit 2 with no heavy
task effects. The prior host binary passed the same rollback checks. Installed
host routing checks passed. Backups, hashes and the acceptance receipt are at
`~/.cache/effigy/rollbacks/runtime-67748464-mcbr1_7z/rollout-receipt.json`.

On installed `v0.13.1+local.6774846`, the previously failing legacy
`effigy exec true` exited 0. Read-only inspection confirmed that Composer cache
remains a host bind from `/Users/tom/.effigy/shared/composer-cache` to
`/home/dev/.cache/composer`; the resolved numeric user `501:20` sees it as
readable and writable, with `501:20` ownership and mode 0755. The legacy route
returned HTTP 200. This proves the reported launch failure is resolved on the
existing running stack; it is not a fresh-stack bootstrap claim. No mount
restructuring, manual chown/chmod, reseed, stack restart or VM restart occurred.
