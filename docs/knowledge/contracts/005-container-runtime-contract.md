# 005 - Container Runtime Contract

Owner: Platform
Last Updated: 2026-10-16

This contract defines the required runtime guarantees for container-backed
task execution in Effigy.

It covers the execution surfaces that depend on a running local container
environment, not production deployment export.

Kernel supersession (2026-10-18): the runtime kernel contract
[052](052-runtime-kernel-contract.md) supersedes this contract's lifecycle and
ownership sections for v0.15: activation and session ownership, partial
activation and recovery, `data_from` handover, owned service start after a
Colima restart, managed host listener lifecycle, owned completion and stop
authority, fallback ownership and failure semantics. Until the environment
lane reconciles them, those sections describe current behavior, not target
design.

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

## Catalog paths in standard commands

For a standard string `run` command, `{repo}` and `{project}` identify the
selected catalog root in the command's execution filesystem. A host task uses
the host catalog path. A host-dispatched container command maps that catalog
through the owning runtime's host root and container working directory,
including the relative catalog suffix. Both aliases and plain task environment
values are rendered with that mapped root before shell quoting and transport.
The same contained `CwdMapper` boundary governs raw exec working directories;
an unmapped catalog path refuses rather than sending a host path into the
container. A catalog that owns its own system uses that system's namespace.

Human and JSON selector plans render the command with this runtime path. The
plan's catalog and `cwd` fields remain host-side selection evidence; they are
not an assertion that a host directory exists inside the container. Task
schema and env-file discovery, cache inputs, configuration and run-control
paths remain host-rooted. Literal command text, user arguments,
secret values and bundle/skill source paths are not rewritten by replacing a
host-path substring. This contract does not promise that a selected service
has mounted a path merely because a command was mapped successfully.

An invocation already running inside a container handoff uses its locally
resolved catalog path. Legacy handoffs retain their selector and original env
metadata for that inner resolution. Mixed container sequences retain their
existing handoff route; host-owned aggregate task references resolve each
standard leaf independently. This change does not enable inline standard
routing, alter managed command mapping or interpret builtin `exec` raw argv as
manifest placeholders. It transfers no host credentials into a container
and establishes no real Linux or remote cancellation proof.

## Isolated directory mounts and reuse

Catalog `isolated_dirs` entries belong to the immutable rendered closure in
every checkout scope. Each entry is a distinct named volume layered over the
source bind. Workspace rewriting preserves those entries; scope identity keeps
worktree volume names separate from the primary checkout.

Reuse compares each rendered named mount's service, destination, resolved
volume name and read/write mode against a fresh selected-backend inspect.
The comparison consumes the final Compose aliases after workspace compaction
and `data_from` renaming, including destination overrides from later files.
Inspects must prove the current full container identity, project, service,
checkout and scope. Missing or unreadable inventories are unavailable, never
clean and never recreation authority (kernel R2, R3 and K8).
Pre-checkout-label stacks use the same legacy admission as reactivation: an
unscoped checkout, matching immutable container identity and a positive bind of
that checkout at the declared working directory. Foreign binds and scoped
unlabelled stacks remain unavailable. External volumes without explicit names
use their bare Compose keys.

Task preparation no longer skips `up` solely because the primary runs. Mount
drift enters ordinary owned-stack reconciliation; recorded detached activation
also checks mounts before skipping Compose. Existing holder exclusion and
reactivation ownership checks remain mandatory (R4). At this pre-spawn boundary,
the incoming ordinary task's own exact-process gate is not yet a bound container
workload and may reconcile its mounts. No ancestor, other process, managed-run
gate or unknown holder receives that exemption. Permission preflight
checks the named mounts again before repair, including explicit exec paths:
a declared volume absent from a live stack cannot grant chown authority over
the host bind beneath it. Permission target metadata uses the most specific
declared mount and retains its repair authority.
An unavailable mount observation preserves the running stack's reuse path.
Permission preparation then verifies access without creating directories,
changing ownership or modes, or creating lock probes; paths that are already
usable remain usable. A concrete access failure still blocks readiness. The
unavailable observation itself cannot request reconciliation, and an up under
live holders keeps `--no-recreate` unless observed drift passes holder exclusion.
An absent service has no live mount inventory; its absence does not prove mount
drift or require recreation. Ordinary up may create it with `--no-recreate`.

`container status` exposes `mount_drift` with a `state` and `details` in JSON
and text; doctor reports drift or unavailable observations read-only. Legacy
stacks need no manual marker removal or volume deletion: their next guarded
`up` applies the rendered mounts. No kernel API or identity migration is added.

## Optional workspace mounts at creation

The operator-authorized Acowtancy feedback on 2026-10-11 requests that optional
workspace extra mounts be resolved for the actual container creation, after
preceding preparation steps, with an included-or-skipped decision diagnostic
at that boundary. Configuration loading must not describe a mount as skipped
by a container that has not yet been created. Required catalog-member mounts
keep their strict validation; missing optional sibling paths remain optional.

`container up` re-resolves the workspace mount spec after runtime and shared
service preparation, immediately before dispatching the primary Compose up.
Legacy string and structured mount forms keep their existing source, target,
and option meanings. Included sources are canonicalized through the existing
mount rewrite and boundary checks. A missing optional source is omitted from
that create spec; catalog members remain required. Policy, status, and doctor
loads do not emit runtime skip diagnostics. They may render a snapshot, but
that snapshot is not evidence about a later creation.

For direct Compose, if the primary service is already running, Effigy keeps the
loaded workspace mount spec for this `up`; it does not refresh that spec in a
way that could recreate the running service just to apply an optional mount.
No creation-time mount decision is reported for that existing service.

Direct `compose_file` containers get the same checkout identity as generated
ones. Effigy includes an ownership override in the immutable closure, at
`checkout/.effigy/runtime/compose/<container>.ownership.compose.yml` relative to
the generation root. It labels each
service the file declares with `com.effigy.checkout` (the canonical checkout)
and, when the checkout has a runtime scope token, `com.effigy.scope`. The repo
file is not edited, and the override adds labels to no service the file does not
declare. Owned-binding checks still require exact full-ID, project, declared
service, non-one-off, running, checkout and scope matches. A container created
before this override has no checkout label, so those checks refuse it and it is
not adopted. Compose applies label changes on its next `up`, which recreates the
affected service. That `up` is the supported recovery for an existing unlabelled
direct container. Diagnostics name it but never perform it.

Exec readiness applies the same owned-binding check before any probe. The
readiness probe and the recovery restart run only after the running primary
service passes it. An unlabelled, foreign or wrong-checkout primary is neither
exec'd nor restarted; the up reports not exec-ready. The shell and task exec
that follow are gated by that readiness boundary.

No path may stop, restart or recreate an environment's containers while a
live lock holder of a run bound to that environment exists, except that run's
own stop.

Task gates establish liveness (contract 052, task engine adoption): a run
whose gate is held on this host, or whose gate probe is unreadable, is a
holder. A matching owner PID and run token in a session record only narrow the
environment binding; runs record the digest of their managed run token. A
missing, unreadable or stale session record never grants mutation authority.
A live holder whose binding cannot be established protects every environment
in the checkout. Gates are host-only (R1): an Effigy task run inside a
workspace container holds its container's own gate and is not a host holder;
container-mode guests requesting host authority is environment-lane work
(contract 052 §8).

The exec-readiness recovery restart therefore keeps probing with its
post-restart window when a live holder exists, then reports any remaining
readiness failure without restarting. Compose up under a live holder uses
`--no-recreate`, preserving existing containers while starting missing siblings.
Lifecycle EXIT traps, including inline-workspace traps, only write lifecycle
state. Runner finish owns exactly one down per managed stop; parent death gives
an orphaned lifecycle shell no teardown authority over a successor.

Failed-generation recovery requires the environment's task/profile locks to
be free of live holders, including a successor with a different run token.
Pre-spawn managed recovery holds its newly acquired task/profile locks and
exempts only that local owner; it refuses any other live holder before down.
A nested `container up` cannot make that exemption. An expired lease reaper
also refuses before any shutdown effect while a task or managed holder remains
live, retaining its lease on refusal. Handover first ends known bound owners
through `effigy <task> stop` and waits for their complete settle, then checks
locks again before touching the losing environment. An unbound live holder
refuses handover; the incoming run's own locks are exempt from that check.
The runner's finish marks its cleanup command with its run token so only its
own stop can down the environment under that run's locks. An unrelated explicit
`container down` must wait for the run's own stop too. An ordinary host task
that explicitly calls down may close out under its own ancestor lock; this
does not exempt a managed ancestor or grant authority to a reaper.

An Effigy invocation that already runs inside its own workspace container
(`EFFIGY_INTERNAL_CONTAINER_HANDOFF=1`) never calls host runtime commands.
`container shell --command`, `container exec`, and `exec` resolve the policy
and working directory from the filesystem only and execute the requested
command directly where the caller already is; a bare `container shell`
opens the login shell locally. No `colima`/`nerdctl`/`docker` probe, no
workspace effigy refresh, and no owned-binding recheck runs from inside —
the marker is only ever set by Effigy's own compose exec into the target
service, so the local container is the requested one by construction.
Task routing, suite targeting, and workspace-shell handoff already stay
local under the same marker; this closes the remaining exec surfaces.
The opt-in bounded shell takes the same local path with its absolute
deadline and stream caps intact, rendering the same bounded-exec schema
without discovery, lease, or workspace-user fallback phases.

Ordinary workload entry gives the selected service's owned-binding observation
(runtime inventory followed by strict inspect) a bounded 15-second window.
The initial workload exec probe then has its separate two-second window; the
existing post-restart probe remains 15 seconds. Final ownership rechecks before
ordinary native exec also have a 15-second bound. This allows Colima transport
hops to complete without spending the workload probe window on initial binding.
Only the selected service is inspected to acquire that binding, and every
effect still rechecks and targets its retained immutable full ID. An explicit
caller absolute deadline bounds verification and execution unchanged: no fresh
phase allowance extends it. Unavailable or foreign ownership still refuses;
the longer observation window grants no restart or successor-adoption authority.

The up diagnostic and detached JSON report describe only the Compose create
spec decision and whether the Compose up command succeeded. They identify
included or absent source/target pairs and report runtime mount verification as
`not_performed`. Compose command success does not prove that a new container
was created, that a running container was recreated, or that the runtime
applied a mount. An unreadable source or one removed after the presence check
fails closed. Effigy does not create missing host paths to satisfy an optional
mount. This ruling changes no scheduler credential or admission boundary.

## Managed host listener routes

`[[containers.<name>.host_processes]]` may opt a process into Effigy's
language-neutral listener contract. The manifest selects the process, bind
preference, HTTP readiness request and browser route. Language adapters pass
the bind preference to their server, bind it strictly, and report the actual
socket.

```toml
[[containers.web.host_processes]]
name = "frontend"
run = 'exec ./scripts/start-frontend'
cwd = "."
restart = "on-failure"
restart_delay_ms = 500

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

The adapter receives `EFFIGY_MANAGED_HOST_LISTENER_BIND`,
`EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE`, and
`EFFIGY_MANAGED_HOST_LISTENER_GENERATION`. Port `0` requests an OS-assigned
loopback port. A fixed preference must be bound exactly; collisions fail
startup. After binding, the adapter writes a bounded JSON report to the
provided report file. The generation value must be echoed exactly:

```json
{
  "schema": "effigy.managed.host-listener-report.v1",
  "generation": "child-generation-token",
  "address": "127.0.0.1:4173"
}
```

Effigy treats the report as a claim to verify. It checks the loopback socket
against the recorded supervisor, child generation, listener PID/start
identity, process ancestry and exclusive kernel socket ownership. Only an
owned listener that passes the declared HTTP readiness request receives a
gateway route. A successful HTTP response, open-port probe, free-port check,
or report by itself cannot publish a route.

Routes publish to the caller's private gateway when
`EFFIGY_GATEWAY_PRIVATE_STATE_ROOT` selects one, and otherwise to the shared
operator gateway. A private root keeps its ownership and permission checks on
every use; the shared gateway directory is the operator's own state and is not
validated as a private root. Route claims, generation binding and owner-checked
withdrawal are the same in both.

Gateway lifecycle gating distinguishes reuse from authority. Route publication
and withdrawal write the owner-checked route table directly and never gate on
the daemon's identity, and a managed task start only reuses an already-running
daemon or starts one. A gateway with an authenticated record, a confirmed
running process, and an identity unreadable to this user (permission-denied) is
therefore reusable on every publish/withdraw/reuse path, including a headless
managed start, which prints one note that the identity could not be verified
and reuses the daemon without replacement — the same way the workspace handoff
path already reuses it. Operations that signal, stop, replace, recover, or
start a second gateway still require a verified identity and keep their
refusals; no new privilege, helper, or root-process read is added.

Dependencies use `depends_on` and must name a declared managed listener. They
start after that listener is ready. A dependent process receives
`EFFIGY_MANAGED_HOST_<NAME>_INTERNAL_URL`, `_PUBLIC_URL`, `_STATE_FILE`, and
`_GENERATION`; `<NAME>` is ASCII-uppercased with punctuation converted to
`_`. Effigy reserves the `EFFIGY_MANAGED_HOST_` prefix. If a dependency
restarts or loses readiness, Effigy stops the dependent generation and starts
it with the current endpoint values.

After manifest resolution, host policy rendering builds one in-memory kernel
`objects::Closure` containing
Original and effective declarations, Compose inputs, generated Dockerfiles/configs,
shared-service Compose, workspace
and DNS rewrites, and inline workspaces. It writes no files, Git exclusions,
scope tokens, ports or loopback assignments. The render's digest determines its
prospective `~/.effigy/state/objects/<digest>` path. An unprepared scoped checkout
uses an explicit `-wt-unprepared` or `-ec-unprepared` project suffix and never
queries its primary checkout's project. Prepare establishes its persistent token
and resolves the final isolated identity.

Manifest resolution reads per-checkout source pins and immutable bundle source
generations (contract 020). Observe never acquires the source cache lock or
fetches a bundle. Existing materialized bytes remain readable during migration;
Prepare imports them and `effigy bundle sync` explicitly updates selected pins.

The Prepare stage of `up` and other Mutate commands publishes through kernel
Objects under the environment owner gate. Files are sealed 0400 and directories
0500. Compose receives `EFFIGY_RENDER_ROOT` for references to generated assets;
this adapter-owned value overrides a caller's environment without changing the
process-global environment. Internal policy readers expand this reserved root
before parsing bind syntax or estimating mount budgets; the immutable file bytes
retain Compose interpolation. Prepare establishes owned mount directories before
admission, so creating a cache cannot change canonical bind paths afterward.
Relative generated asset paths, including `./` spellings, name files in that
same generation. Relative user build contexts and Dockerfiles keep their
Compose meaning. `compose.override.yml` and direct `compose_file` inputs
are copied into the closure. Existing mutable generated files are not imported
or rewritten; the next Prepare renders their replacement automatically.

`effigy container render [<env>]` is read-only and prints every closure file,
its generation path and digest. The path may not exist before Prepare. JSON
uses `effigy.container.render.v1` with `environment`, `path`, `digest`,
`compose_files`, `environment_variables`, and `files` (`name`, `path`, `content`).
Missing allocated ports remain explicit placeholders until Prepare. Scope
queries also leave an unprepared checkout's identity absent. Logs discover the
selected backend's service container directly, including stopped containers,
without opening an unpublished render. Activation stages share one prepared
policy snapshot and return it to exec, deferred tasks and workspace handoff,
so later commands, probes and leases use its final identity and paths.

Detached admission records a v3 generation reference. Verification reads the
kernel's desired object reference and verifies its complete immutable closure;
it no longer fingerprints mutable declarations and renderer outputs. Canonical
checkout identity and scope remain fenced, including a checkout moved behind a
symlink. Source edits leave an admitted snapshot intact; a later Prepare publishes a new desired
generation and supersedes that admission. One-release v2 receipt import checks
its original identity and fingerprints once, then writes a v3 reference in
Prepare. Retained admission is validated before moving the desired reference;
a refusal leaves the prior reference intact. Attached workflows use the same
immutable render path. Execution-only workspace builder recovery compares fresh
source render references while retaining its original stack admission.

Reset and bootstrap fresh-session teardown resolve Prepare inputs and publish
the selected generation before planning Compose or removing generated images,
including on fresh checkouts and after declaration changes. A scoped checkout without a runtime identity performs
no reset effects and does not mint an identity. Every Compose spawn, including
shared-service startup, supplies its immutable generation root to the child.

Reset then drops only the selected environment's object references through kernel
`Objects::reset`; physical reclamation is kernel retention's responsibility.
Sibling environment references survive. Eject exports the resolved closure to
`infra/dev` and does not require a previous `up` or remove admitted objects.

The focused `test:containers:generations:cli` proof snapshots bytes and filesystem
metadata around status, hosts, scope, plan, render and logs for direct Compose
and pinned bundles. The operator-only `test:containers:generations:consumers`
selector requires Python with PyYAML and the cbs/Acowtancy checkouts. It archives
the pre-cutover main renderer into a private temporary fixture, copies the
consumers' evaluated inputs, bundle bytes and host configuration, imports those
bytes through source Prepare, then compares normalized Compose with the pure
CLI's render of the same pins. It never updates a real consumer or its cache.
Engine tests cover concurrent render/publication with admitted readers, refused
admission preserving the desired reference, and reset preserving its sibling.

A recorded *partial* activation reconciles only against its original admission;
changed inputs refuse until `effigy container down`. With unchanged admitted
inputs, a missing or stopped primary does not require operator intervention:
verify every existing project container's checkout and scope ownership, including
stopped siblings, then recover through ordinary Compose up. Unavailable inventory
or foreign ownership still refuses and names the owning checkout's
`effigy container down` command. A *completed* activation
writes a completion marker beside its record. When its inputs later change, or
when a running stack has no record at all (a stack started by an earlier
release, by a container task, or whose record write failed), detached `up`
first proves that every project container - running or stopped, because a
project-wide Compose up can start or recreate a stopped declared sibling -
belongs to this checkout and scope, then retires the stale record and admits
the current inputs with an ordinary Compose up. A container with no checkout,
working-directory or scope label is adopted only on positive evidence: it must
carry this project's exact full identity (listed name, project, declared
service, running, not one-off) and bind-mount this checkout at the policy
working directory. A stopped container is proven by its checkout and scope
labels; an unlabelled stopped container is not proven. Absent labels are not
proof by themselves. Anything not proven refuses with no Compose up, restart or
publication, and names `container down` for the owner.

Operator ruling (Tom, 2026-10-15, v0.15.0 pre-release audit): "Restore for
complete stacks". Detached `up` must not refuse a complete stack whose inputs
changed or that predates the admission record; the strict no-adopt refusal stays
only for recorded partial activations.

Exec readiness keeps v0.14.1's soft budget: the 2-second and 15-second windows
decide whether another probe attempt starts, and each attempt (ownership
re-inspection plus exec) gets its own 10-second bound. Owned-binding
verification likewise allows 10 seconds, because VM-backed runtimes such as
Colima route every call through an SSH hop. A caller-supplied deadline (bounded
commands) stays absolute. Detached JSON
retains closed-vocabulary managed-listener readiness observations (including HTTP
status mismatch) without copying backend output, process names or arguments.
Inventory, exec readiness and exposure failures retain partial/unavailable
state and do not tear down or restart running services. Reconciliation with
`effigy container up --detach` verifies the exact canonical source/context and
fresh owned full-ID bindings before registering exposure. Unchanged admitted
inputs and verified checkout/scope ownership allow Compose up when any declared
service is missing or stopped, including the primary. A complete running stack
reconciles exposure without Compose up. If its recorded managed owner died
with failed or incomplete settling, the next managed run or detached `up`
first verifies admission and checkout/scope labels, brings the environment
down with volumes kept, and releases that generation's attributed locks.
It then starts normally, even when the primary had remained running. Changed partial-admission inputs and
unavailable ownership observations still refuse before Compose effects.
The context persists as the detached owner after success, marked complete; only
the selected temporary task lease is cleared. Explicit `container down` clears that context
and keeps volumes. Attached bring-up keeps its existing cleanup policy.

`container up` runs workspace permission preparation as an integration stage
once exec readiness is verified, before secrets, gateway exposure, and
readiness reporting. `up` is the one stage every first creation, reuse, and
handover passes through; managed children and `container shell` enter through
exec without their own preparation. Owned-disposable cache and build volumes
(Cargo registry/git, target dirs) are therefore writable by the workspace
user however the stack was created. A preparation failure refuses closed
with the container named: detached `up` records a partial activation
without tearing the started scope down, attached `up` runs the ordinary
failed bring-up cleanup, and neither retries, recreates, or adopts anything.

Managed concurrent children share the task's readiness budget
(`health_wait_timeout_secs`, default 60 seconds) for container exec
readiness instead of a fixed guess, with the same per-probe windows as
`container up`. Children wait while the lifecycle state reports `starting`
(the lifecycle writes `started` once `up` — including preparation —
succeeds, and `ready` after readiness waiting) and only then poll the
readiness probe. A slow-but-healthy first start therefore no longer times
children out while the stack is still coming up, and reuse or handover
starts from prepared volumes.

Detached JSON failures contain bounded structural details: state, phase,
unverified readiness/exposure, and supported recovery/stop commands. No raw
backend logs, argv or environment enter these details. Current inspect must
verify full ID, project, service, explicit non-oneoff running state, canonical
checkout ownership and published-port associations. These are fixture-qualified
production adapters, not evidence of the original consumer incident or real
Linux/containerd preservation.

Detached `container up --json` includes `managed_host_listeners` entries for
listeners that passed readiness. Entries report status, runtime and child
generations, assigned address, internal and public URLs, the declared route
domain, the effective route domain, TLS setting, and the versioned state-file
path.

The declared route domain is the manifest value. The effective route domain is
the name every Effigy surface uses: readiness `Host`, TLS certificate
preparation, gateway publication, `public_url` and dependent endpoint
variables. A linked worktree or marked ephemeral clone rewrites the apex with
the same checkout host-key rule as container DNS routes, once, during policy
resolution. The primary checkout and an explicit `share_runtime_identity = true`
keep the declared name. Scope is the checkout generation token, never a port,
process or log. A manifest that declares the same host as both a static DNS
route and a listener is still refused. Listener-only declarations appear in
`container hosts --json` as declared-to-effective HTTP routes. The route table
still refuses a live foreign owner of the effective name. The state file is the ongoing
status source: `starting`, `ready`, `unavailable`, `failed`, or `stopped`. A
child restart keeps the runtime generation and changes the child generation
and address. The internal URL uses HTTP to the actual loopback socket. The
public URL is `http[s]://<domain>`; the gateway terminates TLS when `tls = true`.

Before a child is stopped or replaced, its state becomes unavailable and its
exact route generation is removed. The gateway checks process identity and
socket ownership after connecting and before forwarding HTTP or WebSocket
bytes. A foreign process reusing the old port therefore receives no request.
Stale stop callbacks can remove only their route generation. Teardown signals
recorded supervisor/child generations and removes only their recorded routes.
Missing or uncertain identity remains held; it is not replaced by age-based
cleanup.

Readiness is one bounded HTTP/1.1 `GET` on the owned loopback socket. Effigy
reads the first status line and accepts only the configured status. The line
must be `HTTP/1.<digit>`, a space, a three-digit code, and a space before the
reason phrase. The reason phrase may be empty and may contain only tabs,
spaces, visible ASCII and non-ASCII bytes; control bytes make the line malformed. The request
side stays open after the request; `Connection: close` ends it, and an ordinary
server may drop a response it sees as abandoned after EOF. A status line split
across writes is read to its line break within a bounded response deadline.
Connection failure, write failure, EOF or silence before a status line,
malformed status and any other status keep the listener not ready. The probe
does not retry within one attempt; the startup deadline governs repeated
attempts.

A failed generation keeps a `diagnostic` object in its state file after report
and child cleanup. It records the last startup observation for that exact
generation: `report` (`not_observed`, `absent`, `unreadable`, `unsafe`,
`schema_or_generation_mismatch`, `invalid_address`, `bind_mismatch`,
`accepted`), `claimed_address` (the adapter's loopback claim, when parsed),
`ownership` (`not_reached`, `no_owner_observed`, `exclusive_owner_observed`,
`supervisor_unavailable`, `supervisor_changed`, `child_unavailable`,
`child_changed`, `identity_unavailable`, `identity_changed`,
`ancestry_changed`, `ancestry_unavailable`, `listener_outside_generation`,
`socket_inspection_failed`, `exclusivity_unproven`; `*_unavailable` means the
identity could not be read now, `*_changed` means it was read and differs from
the recorded identity), `candidates_inspected`, `observed_listener_pid`,
`http_probe` (`ready`, `connect_failed`, `write_failed`, `no_status_line`,
`malformed_status`, `status_mismatch`, or absent when not attempted),
`http_status` (only for a parsed status line), and `route` (`not_reached`,
`prepare_failed`, `ownership_changed`, `publication_failed`). The object has
exactly these eight fields. It keeps no response body, headers, environment, or
credentials, and it is not replaced by a later observation until the next
generation starts.

The diagnostic is evidence, never authority. `observed_listener_pid` is a
non-authoritative number; the identity fields remain null for a failed
generation, and no process is signaled, reclaimed or published from a
diagnostic. A value is what was last seen, not current state; read the
top-level `status` for current state. `not_reached` and `not_attempted` mean the
phase was not observed, not that it succeeded or failed. Ownership observations
depend on the Linux procfs or macOS libproc interfaces described below.

### Managed listener socket ownership

Socket ownership uses Linux procfs and macOS libproc interfaces.
On macOS, the same-user process inventory from `ps -Ao pid=,uid=` may print a
UID in signed 32-bit form (for example `-2` for a system daemon). Effigy maps
that presentation to its unsigned UID before comparing it with the effective
UID. Any other malformed or out-of-range row fails the inventory closed. Other
platforms fail closed when process or kernel socket ownership cannot be
proved. `target_host` routes remain static external targets and do not gain
managed-child ownership.
The macOS `PROC_PIDLISTFDS` data query distinguishes a successful empty list
from libproc's zero-on-error result by clearing and immediately capturing the
calling thread's errno. A live descriptorless same-user process does not make
an otherwise exclusive listener unverifiable. Permission and unknown errors,
partial records and out-of-bounds results still refuse inspection. Socket-detail
queries require one complete fixed-size record: a zero result preserves the
native error, and only the existing exact `EBADF`/`ESRCH` race policy can skip
a vanished descriptor or process. Readiness deadlines, UID selection, listener
identity/ancestry checks and repeated exclusivity verification do not change.
The return contracts are pinned to Apple XNU
[`proc_pidfdlist`](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/proc_info.c)
and the public
[`libproc` wrappers](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/libsyscall/wrappers/libproc/libproc.c).
Private native controls retain a genuinely descriptorless owned child through
the production exclusivity scan, preserve invalid-descriptor errors and settle
that exact child before returning. This counterexample does not establish the
cause of historical CI failures whose native inspection error was not retained.
Existing container defaults and non-listener host-process behavior are
unchanged; core does not select a framework or add a built-in dev verb.

## Bounded finite container command ruling

Tom's standing bounded hybrid-profile approval and deliberate owner coordination
on 2026-10-11 authorize an opt-in, language-neutral finite container-command
capability. Existing captured shell commands collect output before emission;
finite streaming surfaces exist but do not expose a native caller deadline and
independent stdout/stderr budgets. An external reader cap cannot bound an
intermediate captured `Output`. This ruling specifies a required capability,
not an available invocation or a retrospective consumer diagnosis.

The caller must select the checkout, project/profile, service, user and working
directory through the existing supported resolution. One absolute deadline
covers discovery, preparation, spawn, eligible identity fallback and final
draining. Independent hard stream budgets apply before collection; their
accounting must remain bounded across any eligible fallback. Both streams drain
fairly without unbounded buffers or reader joins. Overflow, truncation, read
failure, incomplete output or deadline cannot report success or trigger fallback
or retry. Existing command-exit distinctions and identity-fallback eligibility
remain intact; unrelated interactive and default execution retain their
semantics.

Cancellation terminates and reaps only the exact owned host exec process group.
Local client termination is not proof that a remote container command stopped.
Remote cancellation and effects remain explicitly uncertain unless separately
proved through a supported mechanism. No guessed remote PID, service kill,
automatic repeat or cleanup authority follows from that uncertainty. Diagnostics
must exclude raw output, commands, configuration and credentials; supported
results are bounded and incomplete capture remains an error.

Private generic production-path fixtures, independent review and existing CI
must qualify the public spelling, result/error contract and platform limits.
Coherent main assurance and exact-source/platform checksum provenance precede
consumer admission. No consumer runtime attempt, binary replacement, release,
default migration or scheduler authority is granted by this ruling.

### Supported bounded finite command surface

`effigy container <NAME> shell --command <CMD> --bounded --deadline-ms <MS>
--stdout-limit-bytes <N> --stderr-limit-bytes <N> [--service <SERVICE>] [--json]`
is the opt-in invocation of that capability. All four bounded flags are
required together with `--command`; each bound is strictly parsed as a positive
integer and validated before any effect (`deadline_ms` 1..3600000,
`stdout_limit_bytes` and `stderr_limit_bytes` 1..67108864). A missing, zero,
negative, fractional, out-of-range or misplaced bound is refused at parse time;
there is no partial-bound execution. Flags without `--bounded` are refused so
the default command shell keeps its existing unbounded behavior.

While the bounded option is present, the operation enforces one absolute
deadline created before discovery and one cumulative stdout/stderr retention
budget. Runtime liveness and identity discovery, primary workspace Effigy
identity/architecture probes, artifact build or download, service copies and
install captures all use the same bounded reader and consume that same budget.
Their network request and process execution use the remaining absolute
deadline; the deadline is checked before each spawn/request, through process
draining and at phase boundaries. A phase that returns after expiry refuses
without starting the next phase or user command. Local manifest and artifact
selection checks are synchronous and have no command-output capture. Local
filesystem calls cannot be interrupted mid-call; the shared deadline is checked
before the next external operation and at phase boundaries.

Workspace identity refresh keeps the existing primary-service repair selection,
but the bounded runtime may refuse a repair that the plain shell would have
more time to complete. The repair uses the operation's remaining deadline and
budgets; bounded refusal does not fall back to unbounded shell preparation or
launch the requested command.

Both streams drain concurrently into bounded buffers; a stream that produces
more than its remaining budget is an error even when the client exits `0`, and
the retained prefix is never reported as success. Preparation captures and
the eligible existing missing-workspace-user fallback draw from the same
remaining per-stream budgets, so neither preparation nor a second attempt can
reset the hard total. Preparation bytes count toward the whole-operation
`retained` totals, but returned `stdout`, `stderr`, `stdout_bytes` and
`stderr_bytes` contain only the completed user command capture. JSON success
and failure payloads add `preparation.captures` and
`preparation.retained.{stdout_bytes,stderr_bytes}`; failures identify their
stable `phase` and the `owned_host_client_group` termination scope. These are
additive fields in schema v1; existing command-output fields retain their
command-only meaning. Preparation retained counts are bounded subprocess
stdout/stderr, not downloaded artifact bytes. Overflow, read failure, incomplete output and deadline
refuse before any fallback or repeat. The client is launched into an exactly
owned process group; on refusal Effigy terminates and reaps only that group
with a fixed bounded SIGTERM-to-SIGKILL grace that does not rebase the command
deadline.

Selection stays on the existing supported resolution: checkout, named
profile, service and working directory resolve exactly as the plain command
shell resolves them, and the same workspace-user identity is attempted first.
An inner command exit distinction is preserved: a complete in-budget capture
with a nonzero client status is an `inner_command_failed` result carrying the
inner code, distinct from the bounds/deadline reasons. Credentials are excluded
by the shared secret-env rules; neither a bounds failure nor an inner failure
dumps raw command output, arguments or configuration into diagnostics.
Human mode emits retained command stdout/stderr only after a successful inner
exit; a nonzero exit returns safe failure details without writing captured
bytes.

`--json` is accepted only because the bounded mode is non-interactive. It emits
the versioned `effigy.container.bounded-exec.v1` payload with `ok`, status,
bounded command bytes and counts, the declared limits, whole-operation retained
totals, preparation capture accounting, the attempt count, the workspace-user
fallback flag and a `remote_cancellation: "unverified"` fact. Failure payloads
carry a stable reason, phase, stream name when applicable, limit and safe
counts. A launch failure is sanitized at capture construction and rendering:
human and JSON diagnostics exclude raw argv, command strings, configured
container/service names, output, credentials and host error payloads. The v1
failure payload keeps its `container` and `service` keys and uses the literal
`[redacted]` for launch failures. Other bounded diagnostics also never echo
captured command output or credentials.

Transport refusals additionally carry a versioned `refusal_observations`
block, additive in schema v1 and internally versioned (`version: 1`). Every
fact comes from the original monotonic operation origin/deadline and the
capture's own transitions; no bytes, argv, environment, raw errors, content
digests or names are exposed:

- `operation_elapsed_ms` and `remaining_budget_ms` from the original
  operation clock at one coherent snapshot instant, with
  `preparation_elapsed_ms` (origin to the refused attempt's start) and
  `attempt_elapsed_ms` (attempt start to the snapshot); the attempt fields
  are null when no attempt began. The origin is never rebased and no budget
  is reset or extended for diagnostics.
- `spawn_disposition`: `not_attempted` (refusal before the spawn syscall),
  `attempted_not_confirmed` (spawn failed), `spawned` (a host client child
  was created) or `unknown`. It observes the owned host client only and is
  never a claim that a remote or guest process began. A true pre-spawn
  expiry is not an attempt, and the legacy attempt count keeps its
  call-level meaning: `attempts` is not spawn confirmation.
- Per-stream `stdout`/`stderr` objects with `state` (`pending`, `eof`,
  `overflow`, `read_error`, `unknown`), `retained_prefix_bytes` and
  `read_bytes`, each count carrying explicit `exact`, `lower_bound` or
  `unknown` semantics. Pending readers report lower bounds; EOF-complete and
  overflowed readers report exact retained prefixes; read failures report
  final exact counters for a prefix that was never delivered. An exact zero
  from a real EOF is distinct from unknown, and unknown is never zero or
  proof of no effect.
- `leader_exit_observed`: `alive` only after the existing safe `try_wait`
  window observed the leader still running (returning `None` reaps
  nothing); otherwise explicitly `unknown`. The snapshot never calls
  `try_wait`, so no diagnostic observation reaps the leader, spawns a
  process, or looks past the deadline.

The refused attempt's per-stream retained prefix is the failed-prefix
accounting: it is reported separately from the completed-capture `retained`
totals, is never added to them, and draws nothing twice from the cumulative
caps, so fallback eligibility is unchanged. Equal preparation and cumulative
counts therefore still establish nothing about a failed command's output.
Budget-boundary refusals that never reached a transport (for example an
expired budget before a phase) carry the same versioned block with
`not_attempted` and explicit unknown streams. Phase failures that are neither
transport refusals nor budget boundaries carry no block: they are decided
about completed captures (for example a probe that exited nonzero or returned
unusable identity data) or about non-transport steps, where no refused
capture exists to observe, and fabricating a snapshot there would misreport
spawned work as `not_attempted`.

A stream `read_error` is not inducible through the spawned production path:
`Stdio::piped` builds each pipe internally and a child closing a stream yields
EOF, not a read error. That state is qualified at the production reader
itself, with a real owned reader that delivers bytes and then fails, plus the
refusal-snapshot mapping; producing it inside the production transport would
require new injection authority that does not exist.
Terminating the owned host client group is not proof that a remote container
process stopped: remote cancellation stays explicitly unverified, and no remote
PID is guessed, no service or sibling is stopped, and an uncertain command is
never automatically repeated. The bounded path uses the selected Docker or
Colima runtime invocation and bounds each preparatory capture as well as the
user command and eligible fallback. The plain command shell remains unchanged.

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

A checkout may switch between its container environment and hybrid managed
listener mode in either direction, and between any two of its environments,
as one preflighted handover. Before any effect, `container up` of the
incoming environment evaluates everything that could refuse: route claims,
loopback allocation, manifest, policy, ownership and backend validation. A
route held by another checkout, scope or a non-container source refuses with
no effects. When another environment of the same checkout and scope is live
and conflicts — the pair shares data volumes through `data_from`, or the
incoming environment's route claims collide with the live environment's
declared claims — the live environment comes down whole with the ordinary
`container down` semantics: managed host processes and listeners stopped,
routes withdrawn, Compose containers stopped, all volumes kept, scope
records cleared. Before the live environment comes down, the handover
ends every live headless or TUI managed owner of the same checkout whose plan
is bound to that environment, through the same path as
`effigy <task> stop`, and waits for the owner's real settle: the stop
cascade finishes whole - the owned host processes are gone, the
process groups the owner exec'd into the environment are ended through
the exec adapter (or by the environment's own `container down`, which
the cascade runs), and every task lock the run took is released
through the lock owner-token rules. The wait uses the derived managed
settle budget - a bounded remote cleanup attempt per exec child, the child
shutdown grace, the managed lifecycle cleanup's own `container down` budget,
and environment inspection slack - never a fixed guess. Because the owner's cascade already
brought its environment down, the handover withdraws any routes an
inline cascade could not, verifies the environment is down, and runs
the ordinary down only as a backstop if a live remainder somehow
survived; it never races a second conflicting down. One line names
each managed task ended. An owner bound to any other environment is
never touched. If ending an owner fails, the handover refuses before
stopping the environment and names what is still running; it never
advises the stop that is already in progress. A stale lock from a dead
holder is still reclaimed only by the existing lock rules; lock
ownership is never weakened. One line names the handed-over environment and what
moved.

Headless and TUI runs publish the same owner/session identity and run token.
Final cleanup records an optional completion result before owner locks release;
`stop` checks it so an owner whose `container down` failed cannot report a
successful settle merely because its process exited. Older session records
retain the existing process/lock checks. Remote group cleanup resolves full
container IDs from fresh checkout/scope labels, without an exec-readiness or
published-port requirement. A missing container is already ended; an exec
failure against a still-live exact container remains a failure. Whole-stack
cleanup proves every project container's labels, including stopped services and
orphans, before down and keeps every volume.

A managed run's stop - `effigy <task> stop`, a SIGINT/SIGTERM settle,
or a handover-driven stop - reports settled only after that same whole
cascade finished and nothing the run owns is left. Task runs started by
the managed run's processes carry its settle token in their spawn
environment and record its digest; a dead holder's gate is already free,
and a genuinely live holder carrying the token, or a live foreign holder,
still refuses.
The handover refuses only when ending a bound managed owner or
stopping that environment itself fails; it
then names the failure and leaves the environment recoverable by its own
`up` or `down`. The stop is recorded beside the environment's lease before
the first effect, so its next `up` restarts it instead of refusing an
unknown partial state; a recorded handover resolved by a healthy reconcile
or a down is cleared. An unchanged, ownership-verified partial or stopped stack
also restarts without a handover record; unattributable state still refuses. A
managed listener claiming a domain held by a `source = container` route from
the same checkout and scope replaces it with the listener route in the same
locked transaction, and the container environment may keep running.
Container registration takes the domain back once the recorded managed
listener generation is no longer live, decided by the existing
managed-listener availability and ownership checks. Container teardown
removes only `source = container` routes owned by the caller, so stopping
the old container environment cannot remove a handed-over listener route.

## Shared service data volumes (`data_from`)

A container environment may declare `data_from = "<other-env>"`. Every service
the two environments declare with the same name and the same catalog entry —
excluding host-global `shared = true` services — mounts the owner environment's
named volumes instead of its own. Volume names resolve from the owner's
resolved Compose project identity in the same checkout and worktree scope, so a
linked worktree borrows the owner's worktree-scoped volumes and never the
primary checkout's. Services without a match keep their own volumes. Ports,
published addresses, gateway routes and the Compose project name stay per
environment; a borrowing environment's containers carry their own environment
identity labels.

The manifest layer validates every reference at load: the target must exist and
not be the environment itself, chains and cycles are refused, both environments
must use the same driver, and neither side may use direct `compose_file`
ownership or `share_runtime_identity`. A service name declared in both
environments with different catalogs is refused rather than silently split.

Borrowed volumes are declared `external: true` in the borrowing environment's
generated Compose under the owner's volume names. Compose therefore never
creates, removes or re-labels them, and `compose down -v` cannot delete them.
Before Compose up, the borrowing environment ensures each borrowed volume
exists — creation is idempotent and never touches an existing volume's data —
so a fresh checkout can bring up the borrowing environment first.

The two linked environments must never run their shared data services against
the same volumes at once. `container up` of either environment first brings
the other live environment down whole (its shared data services stop with it,
data kept), then prints one line naming what it handed over. There is no
refusal for a running sibling — switching is seamless; only a failed stop
refuses the activation, and it names the environment, project and failure.

Data safety is absolute in the borrowing direction. `container reset` and
`reset --wipe-data` classify only owned volumes, so borrowed volumes are never
removed; `retire`, `retire --dead` and `ports reclaim` never delete or claim
the owner's volumes, and the borrowing environment's runtime-scope record lists
borrowed volumes as retained, never owned. Retiring the owner's checkout still
removes its own resources as today. Environments without `data_from` keep
their own fresh volumes, and manifests without `data_from` generate
byte-identical Compose.

A linked worktree that does not share runtime identity gets one effective host
map for its declared HTTP routes and TCP aliases. The apex becomes
`<apex>-w<host-key>.<tld>` (for `app.test`, `app-w<host-key>.test`); helper
hosts such as `mail.app.test` stay under that apex. The primary checkout keeps
declared names. `effigy container hosts` returns declared and effective names,
origins, cookie domain and WebAuthn relying-party id. Effigy registers those
effective names on the gateway. It does not rewrite application configuration.
For a container manifest, `dns.base_domain` overrides the first resolved DNS
route as the declared apex for generated TCP aliases and the reported cookie
domain; the same checkout-scoping rule applies to that configured apex.

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

Retirement does not accept a container environment name. `container <NAME>
retire` fails before checkout or token record lookup, runtime inventory,
confirmation, or mutation, including when `--scope` or `--yes` is supplied.
`container retire` selects the recorded generation for the current checkout;
`container retire --scope <TOKEN>` selects only that exact record after the
checkout is gone. A record aggregates all environments and runtime profiles
for its generation, so named partial retirement is not supported.

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

Both allocation paths — generated Compose preparation and gateway
registration — reserve every other project's container-route `dns_ip` through
the one shared routine (`reserved_loopback_ips_for_other_projects`), and a
stored assignment that collides with another project's route is reallocated
instead of kept (Tom's 2026-10-15 papercut ruling, effigy#181). Pruning drops
assignments for stacks that are not running while their routes stay
registered, so without this reservation generation could publish a stack on
an IP a stale route's gateway TCP listener still binds while registration
reallocated elsewhere, hanging the host alias against a dead or wrong
upstream while mapped localhost ports stayed ready. Stale-route deletion for
missing checkouts stays with `effigy container retire --dead` (#305).
Status/doctor alias reporting is static route-table analysis (conflicting
bind tuples, missing/invalid fallbacks); live OS-level listener bind failures
remain daemon-log-only.

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

### Refusal-accounting observation boundary

The bounded-execution receipt's legacy cumulative retained counts describe
completed captures. A failed command can have private partial output without
that output appearing in those counts. Equal preparation and cumulative counts
therefore do not establish zero command output or absence of remote execution;
the recorded attempt count is not spawn or remote-execution attestation.

The 2026-10-14 owning hybrid coordination authorizes a bounded generic
diagnostic extension: additive, versioned structural refusal observations from
the original operation clock, truthful spawn disposition, and independent
stdout/stderr count and completion states. Keep legacy meanings, failure
behavior, deadlines, cumulative byte caps and fallback policy unchanged.
Failed bytes, argv, environment, raw errors and content digests stay private.
Unavailable or partial observations must say unknown or lower-bound explicitly.
That block is delivered as the versioned `refusal_observations` payload
described under the supported bounded surface above. It reports structure from
the refused operation only: it does not capture, aggregate or drain beyond the
existing bounded retention, and it adds no observation subprocess.

The leader remains unreaped while either pipe is incomplete. Pretermination
leader state is reported explicitly unknown unless safely observed without
reaping within the original budget (the existing `try_wait` window); no new
subprocess or post-deadline observation is implied. Host-client group
cancellation does not prove remote cancellation or grant cleanup authority.
Controlled private transport proof is separate from the selected image's
PATH-resolved shell and login-input admission, and the observations cannot
establish the historical deadline's cause, a remote process, or any consumer
diagnosis: they distinguish missing host transport observations, nothing more.
No qualified binary or consumer admission is claimed by this source delivery.

### Partial activation and recovery boundary

The 2026-10-14 Acowtancy report establishes a consumer observation: a scoped
bring-up can time out before route registration despite containers starting.
Sibling exit codes and a recurring transient health-check timer collision were
also reported. The proposed link between the timed-out process, log pipes and
SIGPIPE is unestablished; an exit code alone does not identify the sender or
pipe owner. Current Compose startup already requests `up -d`, and scoped
non-interactive retirement already supports `container retire --yes`.

The requested recovery follow-through must preserve explicit partial-state
reporting, exact project/service/container ownership and fresh published-port
evidence. An unavailable inventory cannot authorize a route or be treated as
an empty or ready stack. A later exposure reconciliation must preserve sibling
routes and running exec sessions rather than require whole-stack recreation.
Any newly qualified recreation of a stopped health-checked service must be
limited to that proven owned service, preserve its data and volume identities,
and refuse unknown, foreign, shared or changing ownership. Existing retirement
confirmation does not authorize agents to delete the reported live stack.

Detached publication retains the originally admitted context across probes and
requires it at the locked route save. A publication refusal does not rewrite
its marker from changed inputs. The checked interval and cooperative-source-
writer limits are owned by
[the gateway route trust contract](033-gateway-route-table-trust-contract.md#detached-admission-at-publication).
Detached JSON also reports `activation_stage` from a closed vocabulary:
`preparation`, `reconciliation_inventory`, `compose_activation`,
`admission_record`, `stopped_service_recovery`, `readiness`,
`runtime_integrations`, or `persistent_owner`. This identifies the checked
caller boundary without exposing backend output, argv or environment. It is
not a cause attribution, an absent-resource verdict, or readiness proof.

Restore that admitted context before supported reconciliation; unavailable
context cannot authorize recreation, cleanup or guessed exposure.

The current health-check recovery interface cannot qualify service-only
recreation. Its inspect snapshot records mount references, but does not bind
them to an immutable service declaration, prove exclusive volume/data ownership,
or carry a supported backend preservation plan. A recurring collision therefore
reports those missing seams and refuses recreation. It performs no volume
deletion; `container retire --yes` removes owned mutable volumes and is not a
restart recovery step. The private production-command-adapter fixture
`recurring_health_collision_refuses_recreation_through_production_adapters`
proves the refusal after one exact transient unload and one retry. Retained file
markers establish only fixture non-mutation, not real engine volume preservation
or live exec/session continuity.

Source and controlled private-process proofs may establish the generic
mechanism. They do not retrospectively explain the reported exits or qualify
an unobserved real Linux backend. Live profile repair, held-resource cleanup,
consumer proof waivers and global installation remain separate boundaries.

Runtime activation planning belongs to `effigy-runtime-plan`. The runner
runtime-prep modules are side-effect adapters for that plan: they may start
the runtime, perform readiness checks, reconcile aliases/routes, and refresh
leases, but they must not invent a separate activation model.

## Port registry recovery

`effigy container ports` is the only supported way to release generated-port
allocations outside `container retire`. `status` and `reclaim` without
`--apply` are read-only. `reclaim --apply --expect-plan <DIGEST>` removes only
entries whose project path is definitely missing and that no scope record,
gateway route, container in any state, published port or bound host port still
references. Its locked recomputation must reproduce the reviewed digest. Any
incomplete inventory or unreadable ownership store holds every absent-path
entry, and the report names the reason. Reclaim never removes runtime
resources, data, volumes, routes, certificates, profiles, scheduler settings or
loopback assignments. Writes use the registry's `ports.lock`, the same lock
generated-compose allocation takes, and preserve unknown registry fields.

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

Guest-local shell and exec resolve only manifest execution coordinates: the
container name, selected service and working directory. They never load host
container policy, resolve host catalogs or mounts, render Compose, allocate
ports, mint checkout scope tokens or probe a host backend. A missing host-side
Compose file or catalog does not prevent an already-running guest from
executing locally. Bounded guest commands preserve the same deadline, output
budgets and JSON schema as host-routed commands.

### Generated host-port preparation

Host-port resolution reads existing `ports.json` assignments without allocating,
locking or migrating them. Missing bindings report `unassigned (assigned on
first prepare)` and render a Compose placeholder that refuses execution until
prepared. Shared-service JSON reports `target_port: null` with that label in
`allocation` until assigned. Explicit fixed host ports remain usable without
an allocation.

Prepare calls the existing gateway-domain `ports::prepare_ports` allocator,
then resolves again before admission and publication. The allocator loads,
assigns and atomically saves all requested bindings under its existing short
registry lock; it performs no runtime inventory under that lock. Existing
assignments, including legacy container-port keys, retain their port numbers.
This remains the legacy single writer until the gateway endpoint-store
migration (#224) imports `ports.json` and owns persistent reservations. Kernel
run-scoped Claims do not implement this temporary persistent allocation seam.

### Bind-source preparation

Policy assembly and validation resolve declared media paths and the shared
Composer cache mount without creating their directories. Activation calls
`policy::mounts::prepare_container_mount_sources` before Compose uses them,
including detached admission and task preparation. Media sources are revalidated
against the checkout at Prepare; other host mounts remain user-owned.

### Loopback inventory and registry exclusion

Policy resolution reads existing loopback assignments without taking the
registry lock, allocating, migrating or probing runtime inventory. A missing
assignment stays absent until Prepare. Project-only legacy keys are read only
when owned by the same checkout and uncontested; Prepare imports them to the
checkout-qualified key while keeping the address. Assignments reserved by
another project's gateway routes are not used.

The host Prepare seam prepares loopbacks before ports, including shared
services and environments with only explicit ports or no published ports.
Existing activation, task preparation and data-workflow entry points all use
this seam. No manifest change is required for existing consumers.

Runtime inventory for loopback reclamation runs before the global registry
lock is acquired (K14). Allocation reloads the registry under that lock and
only reclaims entries identical to its pre-inventory snapshot. Assignments
added or changed during inventory are preserved. Missing or incomplete
inventory permits no reclamation. Existing uncontested assignments require
no runtime probe. The registry remains the legacy allocation owner until
the gateway endpoint-store migration.

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
- a stack that an explicit `effigy container up` left running is persistently
  owned: task activation never leases it, the reaper never shuts it down, and
  only an explicit teardown ends that ownership - every `effigy container down`
  form that stops the stack (`--global` and cwd-scoped included) and
  `effigy container retire` once it removes the scope
- lease reuse must not depend on whether the request came from deferral or
  explicit task routing

This is the required shared contract for warm non-shell container reuse.
Task-shaped requests should reach this contract through
`TaskExecutionRequestBuilder` and a resolved execution plan.

## Selected service pack at workspace handoff

After actual selected-workspace activation (including recreation), workspace
binary preparation also prepares the host-selected compatible installed
**service** catalog pack before opening a shell or handing a task to the guest
policy loader. A receipt from a previous guest generation is never reused. Both ordinary and bounded paths enforce this after
an already-current binary probe; executable identity does not establish pack
parity. Task catalog members (including a member named `silo`) never supply
service fragments.

The host pins the existing typed selection, installed record and verified
content before binary preparation. It snapshots only that retained install,
revalidates the snapshot and rechecks selection/content across transport.
The original local source checkout and a registry are unnecessary. An unhealthy,
incompatible, corrupt or changing selected host pack refuses handoff rather
than claiming baseline fallback is equivalent. A host with no active pack keeps
its existing baseline behavior without installing guest content.

For a selected pack, preparation pins the selected Compose service container ID
from a fresh `ps -q` and rechecks it around every guest command/copy. Missing,
ambiguous or changed target identity refuses before handoff; this preserves the
existing backend ownership checks and does not grant adoption authority.
Every preparation command uses the declared workspace user
and HOME and checks the effective uid/HOME before reading guest state. Missing
identity or home configuration refuses. Actual guest `service pack status`
verification must prove pack identity, version, schema, compatibility and exact
content under that home. A verified match performs no guest copy or install.
Only an empty guest selection may receive the snapshot through the existing
validator and atomic installer. `service pack install --path <DIR>
--prepare-content <sha256:DIGEST>` checks the expected content and definite
absence of prior selection under the activation lock; explicit baseline,
unknown state, a concurrent install, or any unreferenced retained content is preserved
and refused. This guarded mode never repairs existing content.

Exit zero alone is insufficient: preparation verifies the resulting status and
re-reads guest selection before dispatch. Partial copy, failed installation,
malformed or mismatched status, wrong identity/home and changed source refuse
before the requested task or shell. Recovery names `service pack status` and an
explicit supported `service pack install --path <retained-pack>` inside the
workspace; automatic preparation never resets a selection.

Host snapshots have an owned temporary-directory lifetime spanning the copy and
verification. Guest staging requires a successful exclusive mkdir as the
workspace user. Verified installation permits removing only that new staging, followed by a
final selection probe before handoff. Failed or uncertain effects before that
cleanup leave staging held. Neither guest store
lineage nor sibling content is deleted. The bounded path uses the caller's
original deadline, cumulative stream limits and workspace-preparation phase
accounting; transport refusals preserve the existing structural observations.
No captured stdout/stderr, arbitrary argv/env, source paths or raw transport
errors are inserted into catalog mismatch messages.

This guarantees installed-pack parity, not whole-environment parity. Mounted
project overrides retain normal precedence. User-global catalog directories
are ignored in both host and guest contexts. Host configuration, HOME and
credentials are not transferred. No channel
refresh, registry acquisition, binary rebuild or network update is implied by
pack preparation. Private production-path controls live behind
`test:workspace:catalog-handoff` and `check:workspace:catalog-handoff`.

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
- named volumes written by two or more compose or managed services: rust
  caches verify only; other shared named volumes are forbidden. Sharing is
  decided by the actual writer set, not by every reference: a service whose
  compose entry mounts the volume read-only (effective Compose short `:ro`
  or long `read_only: true` semantics, resolved across every declared
  overlay and managed volume) is a consumer, not a writer. An Effigy-owned
  primary volume with exactly one writable service therefore stays
  repairable even when Nginx-style services mount it read-only. A service
  whose access mode is ambiguous or malformed conservatively counts as a
  writer, and the primary's own read-only mount keeps non-mutating
  authority: a writer elsewhere never grants the primary mutation power
- read-only, external, or foreign mounts: refuse mutation; fail closed when
  they are declared rust caches

Dependency volumes (Composer `vendor`, JS `node_modules`) that remain
nonrepairable under the rules above - two writers, external or foreign
sources, or a read-only primary mount - are still probed and verified, never
silently omitted. Preparation checks numeric-user read/write access on them
and refuses readiness when it fails, without mutating them; doctor reports an
actionable `container.workspace-ownership` finding for an unusable dependency
volume instead of a clean bill. Doctor stays read-only: no write probes,
chown, or restarts, and an expired or unavailable observation stays
incomplete rather than clean or a permission fault.

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
and reaped at that deadline (only its own recorded process group). The Colima
direct-exec path keeps that absolute deadline through its uncached service
discovery and the capture it supervises: discovery converts the remaining
budget once, and a spawn that returns after the deadline has its owned process
group terminated and reaped instead of being granted a fresh origin. An
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

## Owned-child exit observation qualification — 2026-10-13

Tom's standing bounded hybrid approval and explicit "OK, coordinate with effigy
to get this done" authorize qualification of the generic owned-child observation
and published dependent-replacement path. This extends the existing owned-stop
repair scope; it does not authorize another consumer attempt or runtime cleanup.

The retained `0412cc7b` Reference attempt selected `owner_unverified`, then both
dependent supervisor logs reported `owned child exit observation was malformed`.
The published dependency-stop path propagated that error before relaunch.
Front/admin listener files remained present as `unavailable`; their retained
identities do not prove current process liveness or death. The owner-verification
subfailure and actual malformed process-table capture are not established.

Qualification must first exercise the actual production path in isolated private
host fixtures: published dependency replacement, new tuple/assignments, native
owned-group settlement and cleanup, with siblings and foreign processes
preserved. Valid active/exited/zombie observations must remain distinct from
empty, malformed, unreadable, deadline and failed inspection. No parser fallback,
longer deadline, fabricated completion or weakened identity predicate may turn
uncertainty into success.

Only bounded structural evidence may be added when necessary: exact owned
PID/boot/start/group identity, capture byte count and digest, UTF-8/token/state
classification, result/exit/deadline/capture category and timing. Arbitrary raw
output, environment, commands and secrets are excluded. Any demonstrated defect
must be corrected narrowly; otherwise the exact missing observation or capability
remains explicit. The existing owned-stop correction is not presumed to cure
this capture without independent controlled proof, exact-head review/CI and
coherent main assurance. No consumer repin or retry follows from this policy.

## Private legacy MinIO fixture distribution — 2026-10-12

### Underlay scope correction — 2026-10-13

Tom's subsequent ruling is: "We're supposed to be dropping minio for silo -
this is such a silly waste of time and effort!" For Underlay profile acceptance,
the private legacy MinIO parity/package prerequisite is withdrawn. Further
MinIO packaging, native qualification and review intake for that prerequisite
stop; the existing source, unqualified artifact and receipts remain preserved.
No deletion, abandonment, runtime cleanup or qualification follows from this
ruling.

The bundle owner now owns an explicitly named Silo container/hybrid proof and
default migration through its normal checked scope. Silo must retain its own
identity and contract; it is not a substitution under a MinIO label. This ruling
does not itself change the shipped catalog, prove Silo acceptance or authorize
release, installation or existing-data effects. The older private-package ruling
below remains historical context, not continuing authority for Underlay's
withdrawn prerequisite.

Tom's recorded ruling is: "Authorize separately bounded private fixture-only
package qualification; no shared catalog/default change or publication."
The ruling selects direction; a checked, approved bounded brief must precede
package construction or runtime qualification.

The shipped MinIO catalog remains unchanged. A private fixture package must
identify the exact upstream server release and verify its downloaded bytes,
plus independently pinned base and client identities. It must preserve the
catalog's server command, environment contract, ports, persistent `/data`,
`/bin/sh` target and `mc ready local` health contract. Record packaging and
client differences explicitly; a new private image is not the unavailable
upstream image, and Silo must not be substituted under the MinIO identity.

Artifact inspection and checksums establish package identity, not service
readiness or consumer parity. Qualification must separately prove the native
Linux server/client behavior, bootstrap, storage operations and owned cleanup.
Any unavailable runtime or incomplete ownership proof remains an explicit
boundary. Consumer profile acceptance and its attempt allowance remain with
the bundle owner; package work does not start or retry that profile.

This scope excludes shared catalog adoption, default migration, publication,
release or tag changes, global installation, existing data or volume changes,
and effects on live consumers, gateway settings or VM lifecycle. Private
qualification cannot establish remote cancellation or unrelated Linux Effigy
artifact freshness.

Tom subsequently answered "Approved" to the separate bounded lifecycle
proposal. After incorporating that scope into the checked, approved fixture
brief, qualification may create exactly one new profile named
`effigy-minio-fixture-dfefc279`. Refuse any pre-existing name or state; existing
profiles and the bundle owner's held scopes are not available for adoption.
The profile is Linux arm64 with containerd and VZ, four CPUs, 4 GiB RAM,
32 GiB data disk and 20 GiB root disk. It must not activate host contexts,
modify SSH configuration, forward an SSH agent, mount host directories, edit
templates, enable Kubernetes or foreign-architecture helpers, or add custom
DNS, gateway, host networking or port forwarding.

Record creation intent and exact owned identities before effects. Startup,
one native proof batch and owned teardown have respective bounds of 300,
600 and 120 seconds, with no automatic retries. Cleanup may stop and delete
only that positively identified newly owned profile and its fixture resources.
Any ownership uncertainty leaves them held. This exception does not authorize
changes to any existing VM, held resource, consumer profile, data or allowance.

## Owned completion observation and stop authority

A bounded process-table observation can be unavailable while Effigy still owns
an unreaped child and its created process group. That unavailable observation
must not abort the existing TERM/KILL sequence or reset its clocks. The original
grace (minimum 100 ms) and fixed two-second force phase remain unchanged.
Production uses the fixed bounded OS observer; tests inject only negative
outcomes and use real owned children for positive completion.

Only positive root-exit and whole-group settlement evidence followed by the
actual owned wait permits reaping and a successful exit status. Persistent
observation uncertainty leaves the generation held. An actual wait failure
remains fatal and held; it is not retried as observation unavailability. A signal
permission refusal remains a refusal unless positive settlement and actual wait
resolve the race. Process IDs and status observations grant no signal authority,
and no signal is sent after reaping.

The retained macOS CI failure proves an observation deadline was reported; its
exact historical interleaving remains unknown. This correction does not change
unsafe/FFI blocks, doctor deadlines, admission, container cancellation or public
APIs. Existing unsafe proof-comment observations remain report-only in the
scoped Rust audit, without a soundness or whole-product certification claim.


### Exit capture evidence and published dependent replacement

The fixed `/bin/ps -p <owned-child> -o stat=` observation remains completion
evidence only. Its 500 ms per-capture cap, 64-byte stdout and 128-byte stderr
limits and original stop-phase deadlines are unchanged. Active state tokens
remain active; only a complete valid exited-state observation can participate
in group settlement and the owned wait. Empty, unsuccessful, invalid UTF-8,
multiple-token, unknown-state and incomplete captures remain refusals. No
parser fallback or absent-PID inference authorizes reaping or signals.

A refusal retains only its typed category, owned PID, exit code, bounded byte
counts and SHA-256 correspondence, UTF-8/token classification and elapsed
milliseconds. Captured output, arguments, environment and credentials are
excluded. Launch, read, overflow, incomplete-output and deadline failures keep
their typed capture category; unavailable retained bytes are not invented.
This private diagnostic change does not change listener-state or transition
receipt schemas.

The controlled public proof publishes both a dependency and its dependent,
selects dependency unavailability in the published phase, then requires a new
dependent generation with the current dependency assignments after actual
owned-group settlement. Scoped teardown must preserve sibling and foreign
listeners. It uses private host fixtures and the production native observer;
it does not certify consumer VM cleanup, real Linux or remote cancellation.
The historical malformed capture from bundle005 was not retained. Its exact
bytes and cause remain unknown; the additional diagnostics and controlled proof
do not establish that historical cause or authorize a consumer retry.


### Reviewed replacement source assurance (2026-10-14)

Task 151 / PR 272 completed at clean synchronized main
`77d22adcbdbca20752720d6671310f13894d219f`. Its tree matches independently
reviewed head `11d8530fda6a653a4375152405130826a5158bbd`: all five review
subjects passed, the reviewer executed all nine covering selectors with 105
cases and no required skips, and all seven exact-head CI checks passed.

The one fresh main QA, `01f42aa0-04dd-4a02-a5ef-69ab19f27465`, passed: 5,038
tests, no failures, 34 skips. Setup, QA and teardown each exited zero; every
owned group settled empty, with no escaped descendants or containers. Capacity
released automatically at `2026-10-13T22:58:20.605Z`. One passing test carried
a nextest LEAK diagnostic: `bootstrap_help_is_command_specific` at 2.071 seconds.
Its process/output lifetime cause is unestablished and is recorded separately
as lead `6920b85a-f6c3-4513-bb31-0a9d58df5743`; it is not attributed to the
historical release-plan leak or bundle005 failure.

One private `build:release` from that unchanged source exited zero. The artifact
is Mach-O Darwin arm64 with SHA-256
`95b16dbf56f507f0593ca205694fc5e667d311eee7e67b4e1d3afc81ba724938`.
Its displayed `v0.14.1` is not source attestation. The qualified handover retains
the original intents, exact source/build correspondence, exits, logs, checksum
and settlement in the owner audit directory
`owned-child-exit-observation/final-main-77d22adc/qualified-source-artifact-handover.json`.
It is a private artifact, not an installed or published release.

Acowtancy and the bundle owner retain admission of their actual next runs. This
assurance does not retry bundle005, adopt or clean held resources, change its
spent allowance, prove the historical malformed capture, or qualify real Linux
and remote container cancellation. Source 156's launchd handoff remains privately
qualified source only; administrator installation, live migration and cross-boot
proof are Tom-only and unexecuted. Older source, QA and artifact receipts remain
immutable history.
