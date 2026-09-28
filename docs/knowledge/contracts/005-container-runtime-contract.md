# 005 - Container Runtime Contract

Owner: Platform
Last Updated: 2026-09-28

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
report success. Success requires that no owned container, mutable volume,
network, isolated route, port, loopback, or TLS certificate remains. A
second retire with nothing left is success, including a shared-only scope
whose durable record is then removed.

Workspace archive must invoke `effigy container retire --yes` while the
checkout still exists. After the checkout is gone, retry with
`effigy container retire --scope <token> --yes`. Failure is non-zero and
leaves the durable record. `bootstrap teardown` is only for `--fresh`
bootstrap sessions, not worker scopes. The intended Paseo hook is
`paseo.json` `worktree.teardown`; Queue `workspace.archive` does not call
retire itself. A consumer that still calls `bootstrap teardown`, checks
only running containers, or returns 0 on residue needs its own change.
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
- the supported backend fallback model
- runner-facing container manager operation ownership
- runtime context facts used by container-backed execution
- runtime activation request/plan/report fields
