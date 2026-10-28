# 005 - Container Runtime Contract

Owner: Platform
Last Updated: 2026-10-28

This contract defines the required runtime guarantees for container-backed
task execution in Effigy.

It covers the execution surfaces that depend on a running local container
environment, not production deployment export.

Kernel supersession (2026-10-18): the runtime kernel contract
[052](052-runtime-kernel-contract.md) supersedes this contract's lifecycle and
ownership sections for v0.15: activation and session ownership, `data_from`
handover, managed host listener
lifecycle, owned completion and stop authority, fallback ownership and failure
semantics. Those sections describe current behavior.

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

Mounts are part of a service's rendered configuration, so they need no separate
change detector. One apply (`compose up -d --build --remove-orphans`, no
recreate flag) converges the stack, and Compose recreates exactly the services
whose configuration changed, mounts included ([053](053-environment-reconciler-contract.md)
§3). A running stack is never inspected for mount drift, and `container up`
adds no `--no-recreate`/`--force-recreate` variant.

An isolated volume overlays a directory inside the checkout, so its host mount
point must exist. Up and routed-task preparation create a missing host
mount-point directory for a named volume of the primary service nested under
the working directory (the isolated `target` and `node_modules` trees) before
the apply. A non-directory or symlink at that path refuses; creation never
replaces a file and never follows a link, and the refusal names
`oxide outputs adopt` as the fix, because an Oxide output link on an isolated
path is the shape that meets it. This is a host-side preparation step, not
permission repair: it changes no ownership or mode.

Permission preflight checks the declared mounts before repair, including
explicit exec paths: a declared volume absent from a live stack cannot grant
chown authority over the host bind beneath it. Permission target metadata uses
the most specific declared mount and retains its repair authority. `container
status` reports the services, their runtime state and readiness; it has no
separate mount-drift field.

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

### Retired `compose_file` (one-release translation)

Direct `compose_file` ownership is retired (Q-004). For v0.15 a `compose_file`
environment is read as project-layer catalog fragments, one per Compose
service, with a once-per-process deprecation warning. Each fragment resolves as
`infra/dev/catalog/<container>-<service>/` (suffixed `-migrated` when the name
would shadow a compiled fragment) and the environment as the
`[containers.<name>.services.<service>] catalog = "<fragment>"` declaration
`effigy container migrate <name>` writes. Policy resolution therefore takes the
generated path for both forms, and the render is identical before and after
migration; only the recorded declaration inputs differ. The Compose file is
read, never copied into the closure, and is not read again after migration.

Translation rewrites Compose-relative paths against the template's
`{{ repo_root }}` (build contexts, bind sources, `env_file`), names each
project volume `{{ project_name }}-<volume>` once for every service that mounts
it, and keeps published container ports. As in any catalog fragment, Prepare
assigns the host side unless `[host].ports` fixes it, and the publish address
applies; a gateway route keeps naming the container port. A declared
`[host].ports` mapping no service publishes is published by the primary
service. Generated Compose attaches `[host].mounts` to services that
bind the repo root, or to the primary service when none does, so the binds
direct mode only described now exist. Everything else a fragment cannot express
refuses with the offending key: top-level `networks`, `configs`, `secrets`,
`include`, `name` and extension fields; volume options; service `extends`,
`networks`, `network_mode`, `configs`, `secrets`, `volumes_from`,
`build.additional_contexts`; long-syntax named volumes; non-TCP or
non-loopback port bindings; Compose `$` interpolation; template syntax; YAML
merge keys. A fragment directory already on disk must hold exactly the
translation, or loading refuses.

Translated services carry generated-Compose identity: every service has the
`com.effigy.checkout` label (and `com.effigy.scope` with a runtime token), and
project volumes carry Effigy's volume labels. The first `up` after upgrade
therefore recreates a direct service, and the old `<project>_<volume>` volumes
are left in place and never adopted. Data, cache and reset commands now apply
to these environments.

`effigy container migrate [<name>]` is a Mutate command. It writes each fragment
(`service.toml`, `compose.fragment.yml`) and replaces `compose_file` in the one
checkout manifest file that declares it, preserving comments and unrelated
keys, then re-resolves and compares the render. A changed render restores the
manifest and removes the fragments it wrote. A foreign fragment directory
refuses before anything is written; a matching one is reused, so an interrupted
migration completes on retry. A key declared in a bundle refuses. JSON uses
`effigy.container.migrate.v1` with `environment`, `compose_file`, `manifest`,
`fragments` (`name`, `service`, `path`, `written`) and `render_unchanged`.
`effigy container eject` is retired with it: generated Compose stays internal
and `effigy container render` shows it.

Readiness is Running plus the service's own health check. A service that
declared `restart: "no"` and exited successfully is complete, not unready, so a
one-shot job does not fail the stack's readiness; a failed exit is unready. A service's declared
`restart:` always wins, `"no"` included; a service whose stack declares none
keeps running because `up` asks the runtime for `restart: unless-stopped` with
its own `update` once the stack is started (effigy#383), so a one-shot job stays
down and a service that stops after a VM restart, a Colima restart or a crash
comes back by itself. The rendered Compose file names no restart policy at all:
a policy there let nerdctl's restart manager settle a container between
Compose's create and start, which failed the create, and it would also make the
Compose config-hash depend on the policy Effigy applied. Effigy never probes exec readiness, hand-starts an exited service or
restarts one to repair it: an unlabelled, foreign or wrong-checkout primary
fails the owned-binding check, and a user's own `effigy exec` reports its own
error (a `working_dir` the image does not have fails that command, not `up`).

No path may stop, restart or recreate an environment's containers while a
run holds that environment, except that run's own stop. A bring-up that a live
run holds is not applied: it attaches the requesting runs to the running stack
and reports the holder and that the edit applies on the next up, so an edit is
never silently skipped. A holder list that cannot be read is unknown, not held:
the bring-up warns and applies (Q-011). The environment
reconciler makes this structural ([053](053-environment-reconciler-contract.md)):
every stop, restart, recreation, reset, handover and idle stop is a transition
that owns the environment's gate and needs a permit, refused while any live or
unknown holder remains. Runs hold an environment through the hold API: task
activation, `exec`, shells and data commands hold through a session run; a
managed run is attached by its lifecycle's bring-up. A holder's own stop is
exempt only once that run is settling. Gates are host-only (R1): Effigy inside
a workspace container holds no host gate and requests host authority through
the channel of contract 052 §8.

A bring-up under live holders keeps the running generation and applies a
changed render once they end. Lifecycle EXIT traps, including inline-workspace
traps, only write lifecycle state. Runner finish owns exactly one down per
managed stop; parent death gives an orphaned lifecycle shell no teardown
authority over a successor.

A judged-failed managed predecessor is settled by the kernel when its successor
is admitted; its environment is not torn down, and the successor's bring-up
reconciles whatever stack it left. Handover first ends known bound owners
through `effigy <task> stop` and waits for their complete settle, then runs a
handover transition of the losing environment, which refuses while any other
run still holds it.

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
without discovery, hold, or workspace-user fallback phases.

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

The stopped-inclusive project inventory (`runtime ps --all`) used by task
preparation and owned-service recovery is label-filtered to the Compose project
and retried once, after a short backoff, only when Effigy's own observation
bound expired. It is a read-only observation, so the retry cannot duplicate an
effect; a second timeout, a refused runtime, a parse failure and a non-timeout
failure are all returned unchanged. Two timeouts name the failure as a runtime
inventory admission failure: the task did not run, and the message points at
`container status` and the task's host route where the repository defines one.
No assertion, timeout budget or ownership requirement is relaxed.

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
address, HTTP readiness request and browser route. Effigy gives the listener
its address, verifies that it answers, and publishes the route. An ordinary
server that listens on the `HOST`/`PORT` it receives needs no Effigy-specific
convention: the answering port is the whole readiness and ownership story.

```toml
[[containers.web.host_processes]]
name = "frontend"
run = 'exec ./scripts/start-frontend'
cwd = "."
restart = "on-failure"
restart_delay_ms = 500

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

`readiness.timeout_secs` bounds only the wait after the listener's port first
answers; a child still building has not opened its port, so the supervisor and
the `up` that waits on it carry no deadline and the child's own exit is the
failure that ends the wait.

The child receives `EFFIGY_MANAGED_HOST_LISTENER_BIND`, `HOST` and `PORT`, and
the process's own `EFFIGY_HOST_PROCESS_PUBLIC_URL` and
`EFFIGY_HOST_PROCESS_INTERNAL_URL`, plus the `EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE`
and `EFFIGY_MANAGED_HOST_LISTENER_GENERATION` that shipped adapter scripts
read. Those two are only for existing adapter scripts; a new host process needs
no adapter, and Effigy never reads that report back. `HOST`/`PORT` are the
listener's own address:
they replace an ambient value inherited from the caller's environment, because
a process reading them must bind the port the published route targets. Only a
value the manifest declares outranks them.

A declared port is the manifest's own: it is used untouched, is never reserved
and must be bound exactly, and a port another process already answers fails the
start naming that port. A `bind` whose port is `0` asks Effigy for one: before
any child launches, Effigy takes the address this process last published (so a
restart or a re-`up` keeps every `internal_url` a dependent resolved) or, when
there is none, asks the OS for a free loopback port; the child receives the
result as its bind, `HOST` and `PORT`, and the `internal_url` fact carries it.
There is no port range and no endpoint-store reservation behind that choice,
and nothing is recorded beside the listener's own state file.

## Host process environment

A host-process `env` value is literal text plus `${...}` references resolved
from declared environment facts; `$${` writes a literal `${`. Names are
validated when the container policy loads and an unknown name fails with the
offending `env` key and the valid names. Values resolve when the process
starts, after Prepare assigned published ports and after every declared service
reported ready. The references are `${service.<name>.host}`,
`${service.<name>.port}`, `${route.<domain>.public_url}`,
`${host_process.<name>.public_url}`, `${host_process.<name>.internal_url}`,
`${domains}`, `${apex}` and `${gateway.https_port}`. A value that is not
knowable yet (an unassigned published port, gateway TLS that is not configured)
fails at start rather than guessing.

Each process also receives the active gateway's CA in `EFFIGY_GATEWAY_CA_FILE`.
`NODE_EXTRA_CA_CERTS` and `SSL_CERT_FILE` are filled in only when the operator
has not exported them and the process does not declare them, so an operator
trust store is never replaced. The gateway contract records which runtimes read
which variable and why. Effigy reads no adapter report: only the listener
address answering publishes the route, and the report path is a write-only
courtesy for shipped adapters, removed when the generation stops. A child that
binds a different address never answers the one it was given, so its generation
stays in the no-deadline phase until that child exits, and the failure names
that address, the readiness request it never answered and the exit code.

Routes publish to the caller's private gateway when
`EFFIGY_GATEWAY_PRIVATE_STATE_ROOT` selects one, and otherwise to the shared
operator gateway. A private root keeps its ownership and permission checks on
every use; the shared gateway directory is the operator's own state and is not
validated as a private root. Route claims, generation binding and owner-checked
withdrawal are the same in both.

A private root's directories are owner-only (`0700`) and its files deny group
and other writes. Creation paths state that mode instead of inheriting the
process umask: the state root's subdirectories and a created gateway parent are
made owner-only explicitly, and the `mkcert` subprocess that writes the CA, the
CA serial file and each certificate pair runs with a child-scoped umask, so a
Linux caller's default `0002` cannot leave group-writable state that Effigy's
own validation then refuses. The files it leaves are pinned to `0600` before
validation. The refusals themselves are unchanged: a pre-existing group- or
other-writable file or directory, a symlink, or a foreign owner is still
refused rather than repaired.

Gateway route publication and withdrawal write the owner-checked route table
without gating on daemon identity. Managed startup reuses a running daemon and
warns when its build differs; only positive absence starts one. An unavailable
observation warns and proceeds without starting a second daemon. Replacement
belongs to explicit `gateway down` / `gateway up`. Fixture commands require a
private root, including stops.

Every operative `gateway up`, `gateway down` and `gateway setup` action appends
one bounded event to `gateway.stderr.log` in the gateway state directory it
targets, when it starts, so an action that then refuses, finds nothing to do or
fails is attributable too. Recording appends to the log the gateway already
keeps and never creates its directory, so an `up` or `setup` on a machine
without gateway state stays read-only. Kernel custody settlement appends the
same record for a machine gateway stop it performs. The event is written at the
command boundary and nowhere else: the shared gateway lifecycle (`operator::up`,
`operator::down`, the signal helpers) appends none, so a `setup` step that starts
or stops the gateway emits its single `gateway_setup` event and nothing more, and
a task run that starts the gateway in-process is recorded with the cause
`managed gateway start` rather than as an operator `gateway up`. One action is
one record, and a replacement or a migration stop stays attributable through the
`up` or `setup` action that requested it. An event keeps the existing `event`, `pid`, `argv`, `cwd` and
`cause` fields, redacts credential-shaped argument values, and adds a `caller`
record:

- the invoking process's start identity (the platform's own process-start
evidence, so two processes that held one pid stay distinguishable), its
session and its controlling terminal when it has one. The terminal is the
session's controlling terminal, resolved from the kernel's own record of it by
device: macOS reads `bsdinfo`'s terminal device and names the pty slave
(`/dev/ttysNNN`), and Linux reads the controlling-terminal device from the
process's `/proc/<pid>/stat` line and names it (`/dev/pts/N`, `/dev/ttyN`). A
standard descriptor is never taken as the terminal, so a redirected descriptor or
one attached to another terminal cannot misattribute the action, and the generic
`/dev/tty` alias is never recorded;
- the parent chain from the invoking process's parent up to and including its
session leader, capped at eight entries, each with pid, start identity, session
and redacted argument boundaries. The walk stays inside the invoking process's
session: a parent in another session — a reparented child, or a leader the walk
cannot reach — ends the chain with a bounded unknown reason instead of naming
another session's leader;
- only allowlisted agent/task/run identity markers, read by name from
`PASEO_*`, `NS_*` and `EFFIGY_RUN_*`, and only when the value is one of the
supported identity forms: a `8-4-4-4-12` hexadecimal UUID, a decimal id, or —
for `EFFIGY_RUN_*` — Effigy's own 26-character base32 run id. A marker is
recorded as its recognised form and a SHA-256 fingerprint, never its value,
because an ID-shaped value can still be a credential: two events from the same
agent, task or run correlate through the fingerprint and no raw marker value is
written. Every other environment value is never read;
- a bounded `unknown` list naming what could not be read: an unreadable or
vanished parent, unreadable arguments, a pid whose start identity changed or
could not be confirmed by a second read, a truncated chain, or a marker whose
value is not a supported identity form. No reason ever quotes the value it
rejected, so a password pasted into a marker variable is refused by name only.
Chain failures keep their place in that list, which is capped at eight entries
and ends with how many reasons it omitted, so a flood of invalid markers can
never hide a missing or unconfirmed part of the chain.

Command text is published by role, not by spelling. The invoking process's own
`argv` is Effigy's grammar, so it records the program's file name and one of
Effigy's own flag names; every other token is `<redacted>` — a positional
argument (whatever its spelling, because a positional password cannot be told
from a command word), a path, an unknown flag such as `--hunter2` or `--token`, a
URL, a `KEY=value` pair, a JSON body, and the value of any flag in `--flag value`
or `--flag=value` form, so `gateway setup --suffix down` records
`--suffix <redacted>`. A flag this record does not define is treated as taking a
value.

An ancestor's command is another program's text, so only its program is
published: the file name of the executable the operating system reports — never
its caller-chosen `argv[0]` — and only when that name is in the published program
vocabulary. No option token of an ancestor is published, not even one that spells
an Effigy flag, because that program's flags and their values are its own and a
token that matches can be a value. A shell command string is read as foreign text
for the same reason, so nothing inside it is published. Attribution therefore rests on the
pid, start identity, session, controlling terminal, markers, `cause` and the
`action`, not on arbitrary text from a command line.

This record is diagnostic evidence only. It never authorizes, refuses or
delays an operator action, and an unavailable log never fails one. The
elevated child of `gateway setup` writes no record, because root would own the
operator's log file; the operator's own invocation records the action and its
chain already names that child.


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
scope tokens, ports or loopback addresses. The render's digest determines its
prospective `~/.effigy/state/objects/<digest>` path. An unprepared scoped checkout
uses an explicit `-wt-unprepared` or `-ec-unprepared` project suffix and never
queries its primary checkout's project. Prepare establishes its persistent token
and resolves the final isolated identity.

Manifest resolution reads per-checkout source pins and immutable bundle source
generations (contract 020). Observe never acquires the source cache lock or
fetches a bundle. Existing materialized bytes remain readable during migration;
Prepare imports them, host Prepare paths refresh a stale pin, and
`effigy bundle sync` explicitly updates selected pins.

The Prepare stage of `up` and other Mutate commands publishes through kernel
Objects under the environment owner gate. Published files and directories keep
Effigy's private modes (0600 and 0700); the digest proves integrity.
A rendered input names the generation's other files relatively, and Compose
resolves them against its project directory — the directory of the invocation's
first `-f` input — so no Effigy value is interpolated into the stack and no
container environment carries one. Internal policy readers resolve those
relative inputs to the generation's own paths before parsing bind syntax or
estimating mount budgets; the immutable file bytes keep the relative spelling.
A shared service belongs to no checkout, so it is not part of a generation. Its
Compose input and every file its fragment binds (MariaDB and Postgres init SQL)
are published at one machine-wide, content-addressed directory under the Effigy
home — `<effigy home>/shared-services/<project>/<digest>/` — with absolute bind
sources. Two checkouts with the same shared declaration therefore hand Compose
byte-identical input at the same absolute path, and its config-hash does not
change with the checkout. That render also names itself on the service it
creates (`com.effigy.shared-render`). `up` reads the label from the runtime: a
shared container from another render is stopped and removed before Compose
creates the new one, so its published host port is never held twice, and its
named data volume is kept. An unchanged shared container is not touched. One
machine gate per shared Compose project serialises the ensure, so two checkouts'
`up`s never interleave a replacement. The render is published by that ensure,
not by `prepare`, so a vanished directory is seen before it is restored. A
container whose own render directory is gone (its `<project>/<digest>` the
runtime labels it with) refuses the ensure before any publish, stop, remove or
Compose call, and the refusal names the container. An unreadable inventory
refuses when the current render is also missing, because no container can be
ruled out. The container and its named data volume are left as they are; the
operator removes that exact container with the runtime's `rm -f`, and a re-run
of `up` publishes the render and creates the service on the preserved volume.
A container whose render is present and differs from the current one is
replaced as before, and a missing render with no container is created.
`build.dockerfile` is the one relative input Compose reads against the *build
context* rather than the project directory, so a catalog Dockerfile is rendered
inline as `dockerfile_inline` from the generation's own content: the stack names
no path outside the generation and a copied or moved checkout renders the same
image.
`container up` renders once: the creation load allocates ports, prepares every
mount source and publishes the generation, and every later stage and the apply
use that one policy. A task's environment ensure runs the same apply as
`container up` — the same shared-service start, the same secret delivery and the
same Compose arguments — so alternating the two paths recreates nothing.

Prepare establishes owned mount directories before
admission, so creating a cache cannot change canonical bind paths afterward.
Relative user build contexts, mapping- and sequence-form
`build.additional_contexts`, and Dockerfiles keep their Compose meaning. A
relative additional context is rewritten against the file that names it, so
Compose does not resolve it against the generation. Remote and already-absolute
values stay as written. A mapping-form `service:<name>` named context stays as
written; sequence form still rewrites that value. `compose.override.yml` inputs are copied into the closure. Existing mutable generated files are not imported
or rewritten; the next Prepare renders their replacement automatically.

`effigy container render [<env>]` is read-only and prints every closure file,
its generation path and digest. The path may not exist before Prepare. JSON
uses `effigy.container.render.v1` with `environment`, `path`, `digest`,
`compose_files`, `environment_variables`, and `files` (`name`, `path`, `content`).
`environment_variables` stays in the contract and is always empty: a render
hands Effigy no value to interpolate.
Missing allocated ports remain explicit placeholders until Prepare. Scope
queries also leave an unprepared checkout's identity absent. Logs discover the
selected backend's service container directly, including stopped containers,
without opening an unpublished render. Activation stages share one prepared
policy snapshot and return it to exec, deferred tasks and workspace handoff,
so later commands, probes and holds use its final identity and paths.

Detached admission pins the kernel's desired object reference and verifies its
complete immutable closure; it never fingerprints mutable declarations and
renderer outputs, and it writes no receipt file. The environment record
([053](053-environment-reconciler-contract.md)) holds the object reference the
environment points at, and every Prepare publishes a new desired generation.
Canonical checkout identity and scope remain fenced,
including a checkout moved behind a symlink. Attached workflows use the same
immutable render path. Execution-only workspace builder recovery compares fresh
source render references against the running stack.

Reset and bootstrap fresh-session teardown resolve Prepare inputs and publish
the selected generation before planning Compose or removing generated images,
including on fresh checkouts and after declaration changes. A scoped checkout without a runtime identity performs
no reset effects and does not mint an identity. Every Compose spawn, including
shared-service startup, supplies its immutable generation root to the child.

Reset then drops only the selected environment's object references through kernel
`Objects::reset`; physical reclamation is kernel retention's responsibility.
Sibling environment references survive.

### Exact backend and resource adoption

Every Compose, volume and data action targets one exact backend, resolved in
this order: `EFFIGY_COMPOSE_BACKEND` for the invocation, the environment's
kernel `resources` row, a v0.14 `.effigy/runtime/compose/.effigy-runtime.json`
entry not yet imported, the user preference, then the driver default. Prepare
records the engine, its context (the Colima profile, or `DOCKER_CONTEXT`/
`default`) and the source that chose it in the same owner run that publishes
the desired generation, before any engine effect. The legacy file is read for
one release and never written or deleted. A record Effigy cannot read, or an
unparsable legacy file, is `Unknown`: activation may fall through to the next
source, but volume and data actions refuse until the backend is named with
`EFFIGY_COMPOSE_BACKEND`, and Prepare records nothing from a guess.
`container status` reports `backend_source` (and `backend_unknown`).

Volume actions run on the environment's endpoint: `docker`, or its Colima
profile. The profile name no longer implies the engine; the exact backend
chooses the endpoint, `EFFIGY_COMPOSE_BACKEND` never redirects the Docker
endpoint, and shared services start on the dependent environment's backend. A volume is adopted
only when Effigy's labels prove it: `com.effigy.managed=true`,
`com.effigy.project` naming the environment's project and
`com.effigy.volume-name` naming the volume. Reset and cache prune remove only
such volumes; a same-named lookalike is reported and left alone, and data
import and export refuse it. A listing or inspection failure is an error,
never an absence (contract 052 R3).

The focused `test:containers:generations:cli` proof snapshots bytes and filesystem
metadata around status, hosts, scope, plan, render and logs for a translated `compose_file`
and pinned bundles. The operator-only `test:containers:generations:consumers`
selector requires Python with PyYAML and the cbs/Acowtancy checkouts. It archives
the pre-cutover main renderer into a private temporary fixture, copies the
consumers' evaluated inputs, bundle bytes and host configuration, imports those
bytes through source Prepare, then compares normalized Compose with the pure
CLI's render of the same pins. It never updates a real consumer or its cache.
Engine tests cover concurrent render/publication with admitted readers, refused
admission preserving the desired reference, and reset preserving its sibling.

Every bring-up is one apply: `compose up -d --build --remove-orphans`, with no
recreate flag, from one render. Compose recreates exactly the services whose
rendered configuration changed; an edit to an image, environment or mount is
applied by the next `up`, with no `down` first. A failed or interrupted `up`
leaves nothing to clean up: the next `up` reconciles from what is actually
running. A live run holding the environment keeps the running stack and is
attached to it; it is never recreated under.

When the record holds no applied render and the backend project owns no
container, network or volume, `container down` is a no-op that renders nothing
and calls no Compose. A stop of a never-activated render still renders and
interpolates with the teardown-only value, never assigning a port; `up` and
`prepare` still fail closed on the unassigned-port refusal.

A missing or stopped primary does not require operator intervention: one
labelled `ps` reports every existing project container, including stopped
siblings, and the same apply recovers the stack. A container whose Compose
project or Effigy checkout and scope labels name another owner is not this
checkout's. A container from an earlier release that carries no Effigy checkout
label is reported, never adopted: `effigy container down` and `up` relabel it.
A profile Effigy created or restarted leaves nothing to repair: the apply
recreates each service whose rendered configuration differs.

Operator ruling (Tom, 2026-10-15, v0.15.0 pre-release audit): "Restore for
complete stacks". Detached `up` must not refuse a complete stack whose inputs
changed or that predates the admission record; a changed declaration is simply
applied.

Exec readiness keeps v0.14.1's soft budget: the 2-second and 15-second windows
decide whether another probe attempt starts, and each attempt (ownership
re-inspection plus exec) gets its own 10-second bound. Owned-binding
verification likewise allows 10 seconds, because VM-backed runtimes such as
Colima route every call through an SSH hop. A caller-supplied deadline (bounded
commands) stays absolute. Detached JSON
retains closed-vocabulary managed-listener readiness observations (including HTTP
status mismatch) without copying backend output, process names or arguments.
Inventory and exposure failures report the activation as
unavailable and do not tear down or restart running services. Reconciliation
with `effigy container up --detach` verifies owned full-ID bindings before
registering exposure. Verified checkout/scope ownership allows the apply when
any declared service is missing or stopped, including the primary. A managed
owner that died with failed
or incomplete settling is settled by its successor's admission (contract 052
K5); its environment is not torn down, and the successor's bring-up reconciles
it. Unavailable ownership observations still refuse before Compose effects.
Activation publishes `active` and makes an explicit `up` the persistent owner.
Explicit `container down` ends
that ownership and keeps volumes. Attached bring-up keeps its existing cleanup
policy, run as a stop transition after the bring-up's own runs close.

`container up` runs workspace permission preparation as an integration stage
before secrets, gateway exposure, and
readiness reporting. `up` is the one stage every first creation, reuse, and
handover passes through; managed children and `container shell` enter through
exec without their own preparation. Owned-disposable cache and build volumes
(Cargo registry/git, target dirs) are therefore writable by the workspace
user however the stack was created. A preparation failure refuses closed with
the container named: detached `up` leaves the started scope in place and the
next activation reconciles from what is running, while attached `up` runs the
ordinary failed bring-up cleanup. Neither retries, recreates or adopts
anything.

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
backend logs, argv or environment enter these details. A failed detached
backend's output now lives in the checkout's retained runtime record
(`.effigy/runtime/logs/<project>.compose.log`, bounded to its last 64 KiB) and
its tail enters the operator-facing error text, which is not part of the JSON
detail vocabulary. Current inspect must
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
exact route generation is removed. Stale stop callbacks can remove only their
route generation. Teardown signals recorded supervisor/child generations and
removes only their recorded routes. Nothing about a listener is re-proved per
connection: the gateway forwards the route to its published target like any
other route.

A published listener stays published while the supervisor's run is alive and
the listener port answers. The supervisor connects to that port every five
seconds; three consecutive misses withdraw the route and record `unavailable`,
and the next answer republishes it and records `ready` again. A republication
that fails while the port keeps answering is retried on the next probe, so a
brief gateway-store failure never leaves a healthy child unreachable: the
generation stays `unavailable` with no route until a retry succeeds. A probe
miss never kills or restarts the child, and never replaces a dependent: the
child's own exit drives restarts, and a dependent is replaced only when its
dependency's supervisor generation changes. A restart, a watch-mode respawn
that replaces the process serving the port, a double fork and a long compile
are all ordinary.

Readiness is one bounded HTTP/1.1 `GET` on the listener's loopback socket. Effigy
reads the first status line and accepts only the configured status. The line
must be `HTTP/1.<digit>`, a space, a three-digit code, and a space before the
reason phrase. The reason phrase may be empty and may contain only tabs,
spaces, visible ASCII and non-ASCII bytes; control bytes make the line malformed. The request
side stays open after the request; `Connection: close` ends it, and an ordinary
server may drop a response it sees as abandoned after EOF. A status line split
across writes is read to its line break within a bounded response deadline.
Connection failure, write failure, EOF or silence before a status line,
malformed status and any other status keep the listener not ready. The probe
does not retry within one attempt. A closed port is not a failure while the
child is alive: the supervisor waits with no deadline and reports progress to
the process log, and the child exiting is the failure. The declared readiness
deadline starts when the port first accepts a connection and then governs
repeated attempts, so a cold build's compile time is never spent against it.

A generation that fails keeps a one-line `diagnostic` string in its state file
after its child is cleaned up, and a settling supervisor does not replace that
evidence with a blank terminal record. The line names the listener address, the
readiness path and status, and the last probe outcome (`connect_failed`,
`write_failed`, `no_status_line`, `malformed_status` or `status_mismatch`); a
start refused because the port already answered names that port, and one whose
child exited first names the exit code. It keeps no response body, headers,
environment or credentials, and it is evidence, never authority: read the
top-level `status` for current state.

The gateway's process and socket interfaces (`socket_ownership`, `macos_socket`
and `linux_net`) stay for the gateway's listener handoff. A managed listener no
longer reads them.
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
The project name scopes Compose containers, networks and managed volumes, not
host-port allocation. Unpinned published ports use Compose or OS assignment;
`stable_ports` opts into a checkout-path-derived host port. In a runtime-scoped
checkout, a manifest's fixed host port is replaced with an OS-assigned value
unless `stable_ports` is enabled, so it does not compete with the primary.
Repeated commands in one worktree reuse the token. Recreating a worktree at the
same path creates a new token. Without `stable_ports`, the primary checkout
keeps its existing project name and explicit-port behavior.

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

Gateway route ownership is the checkout path alone. One endpoint store
transaction is the whole write path: a claim, a takeover and a withdrawal all
commit from it, and no separate lock or generation token is recorded. A live
foreign owner blocks a domain claim, including HTTP, TLS and TCP alias routes.
Withdrawal removes every route the caller's checkout path owns, listener routes
included, and leaves a route another checkout owns. Only a *missing* checkout
path is gone: an unreadable path (permission refusal, a stale network handle)
and a path whose own parent is missing (a volume that is not mounted yet) are
unknown, and their routes stay. Nothing withdraws a route because its target is
briefly down, so a container restart, a Colima restart or a listener that has
not bound yet keeps its route. A missing worktree path is stale and may be
claimed by another worktree. A fixed domain therefore refuses a second live
worktree until the consumer supplies distinct domains. The route table never
silently moves a live sibling's domain.

A TLS certificate is a cache keyed by domain. `down` and `retire` keep it, so a
stack that comes back finds its certificate ready, and the gateway daemon
prunes a certificate no route names once it is seven days old.

A checkout may switch between its container environment and hybrid managed
listener mode in either direction, and between any two of its environments,
as one preflighted handover. Before any effect, `container up` of the
incoming environment evaluates everything that could refuse: route claims,
loopback pool capacity, manifest, policy, ownership and backend validation. A
live route owned by another checkout path refuses with no effects. When
another environment of the same checkout and scope is live
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
container IDs from fresh checkout/scope labels, without a published-port
requirement. A missing container is already ended; a cleanup failure against a
still-live exact container remains a failure. Whole-stack
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
stopping that environment itself fails, or when another run still holds it;
it then names the failure and leaves the environment recoverable by its own
`up` or `down`. The stop is a journalled transition of the losing environment,
so its next `up` reconciles it. An ownership-verified stopped stack restarts;
unattributable state still refuses. A
managed listener claiming a domain held by a `source = container` route from
the same checkout replaces it with the listener route in the same locked
transaction, and the container environment may keep running. Container
registration takes the domain back the same way: the checkout path is the whole
owner, so the last writer wins between a container environment and its
listener. `down` removes the checkout's routes by path, listener routes
included.

## Shared backing-service clients

A shared backing service is reached through environment variables, so those
variables go to the stack's own clients: services whose catalog is an exec
target and neither publishes a loopback endpoint nor is shared-eligible itself.
That is the workspace service and the app services. Catalog infrastructure — a
backing service or an appliance whose own connection it declares — keeps the
environment it declares, so a local MariaDB catalog service never honours the
shared `MYSQL_HOST` in its own health check. A value a service already declares
wins over the injected one.

Each client that receives those variables also carries
`extra_hosts: host.docker.internal:<host address>`. On Colima the address is the
profile gateway address the runtime DNS override maps host gateway domains to,
because that override replaces the VM's own resolver and with it the host
alias. A Docker engine backend supplies `host-gateway` for the same name, which
Docker Desktop and Linux Docker both resolve. A service that receives no shared
environment gets no entry.

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
must use the same driver, and neither side may declare the retired
`compose_file` or `share_runtime_identity`. A service name declared in both
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
removed; `retire` and `retire --dead` never delete or claim
the owner's volumes, because a borrowed volume carries the owner's Compose
project label, not the borrower's. Retiring the owner's checkout still removes
its own resources. Environments without `data_from` keep their own fresh
volumes, and manifests without `data_from` generate byte-identical Compose.

A linked worktree that does not share runtime identity gets one effective host
map for its declared HTTP routes and TCP aliases. The apex becomes
`<apex>-w<host-key>.<tld>` (for `app.test`, `app-w<host-key>.test`); helper
hosts such as `mail.app.test` stay under that apex. The primary checkout keeps
declared names. `effigy container hosts` returns declared and effective names,
origins, cookie domain and WebAuthn relying-party id. Effigy registers those
effective names on the gateway. It does not rewrite application configuration.
For a container manifest, `dns.base_domain` declares the apex for generated TCP
aliases and the reported cookie domain and RP ID; the same checkout-scoping
rule applies to that configured apex. Without a declaration Effigy infers the
base from the HTTP routes: the shallowest route host served by the primary
service (the service of the front route) or by a managed host listener wins,
route order breaks a depth tie, and only when neither serves a usable host
does the shallowest route host of any service stand in. A wildcard route never
becomes the apex, and an inferred base is one of the declared route hosts,
never a parent domain the routes share. So a bundle whose first route is a
subdomain no longer names that subdomain as the apex, while a project whose
services already use a deeper name keeps it. Policy load resolves that one
apex (declared or inferred, in the checkout's scope) and passes it to the
generated TCP aliases, the Compose route-parity `extra_hosts` and gateway
service-alias registration, so `container hosts` never advertises a different
apex than the gateway or the container's own aliases. When an inferred base is
not a suffix of every HTTP route, `container hosts` warns and its JSON
`base_domain` object carries `"inferred": true` and the `warning`. An
undeclared base is inferred; a manifest that needs a different apex declares
`base_domain` explicitly. A declared value is never inferred or rewritten.

Ownership is the runtime's own labels: `com.docker.compose.project=<project>`
plus Effigy's checkout label (`com.effigy.checkout`, or `com.effigy.repo-root`
for a volume the `data_from` path created) and, when a scope is selected, the
`com.effigy.scope` token. Effigy keeps no record beside them: `effigy container
retire` asks the backend what carries those labels and deletes all of it, on
every profile the runtime reports and on the configured endpoint except a
stopped Colima profile, which is a warning: nothing on it runs and it cannot be
read without starting its VM. There is no
runtime-scope journal, no checkout generation and no custody-lease fence.

Listing uses `ps -a`, and each kind is read with one labelled `ps` or `ls`, so
Created and exited containers retire with running ones. A container is stopped
before it is removed, so a `restart: unless-stopped` service is not restarted
while it is being retired. Compose networks carry
no Effigy label of their own, so they are found by their
`com.docker.compose.project` label and attributed to the one checkout the
project's labelled resources name; a project two checkouts share is never
attributed and its networks stay.

`nerdctl compose` copies neither Compose's per-volume `labels:` nor Effigy's
labels onto a named volume: it leaves `com.docker.compose.project` and the name
Compose gave the volume, `<project>-<volume>` (`<project>_<volume>` on Docker
Compose). That name, found through the checkout's own project names, is the
volume's ownership proof, so `retire --yes` removes those volumes instead of
reporting them as unlabelled. A volume of a project whose containers are
already gone is still found, because the checkout's manifest names its
projects. `retire --keep-data` keeps every labelled
volume, names which ones are named data (`com.effigy.persist=true`) and lists
them; without it retirement removes them with the rest of the stack. What the
backend still lists afterwards is named with the runtime's own error and
retirement exits non-zero; a resource already gone is removed, not a failure,
so it never reports a clean result while an owned resource survives.
Retirement also releases the checkout's gateway routes by checkout path alone,
which frees its loopback address with them. Its certificates stay as a cache
the daemon prunes by age. A second retire with nothing left is success and
reports `idempotent: true`.

A container, volume or network of one of this checkout's Compose projects that
carries no Effigy checkout label is reported under `kept` and left alone, never
adopted: run `effigy container down` and `up` to relabel a stack from an earlier
release.

Retirement does not accept a container environment name. `container <NAME>
retire` fails before checkout resolution, inventory, confirmation, or mutation,
including when `--scope` or `--yes` is supplied. `container retire` retires the
current checkout; `container retire --scope <TOKEN>` restricts it to one scope
token of that checkout. For a checkout whose path is gone, `container retire
--dead` groups the labelled resources by checkout path and retires the groups
whose path no longer exists, from any live checkout. Dead means the path is
missing: an unreadable or relative path label is reported under `unknown` and
never guessed about. Retirement never recreates a checkout path.

Resolving, discovering, planning, or reporting container policy does not
register runtime ownership, and no activation writes ownership state either.
Ownership exists only on the runtime objects themselves, so a re-cloned
checkout at the same path retires its stack with no generation or record to
reconcile, and a deleted worktree's stack is still findable by label.

Loopback allocation follows the same ownership boundary. An isolated project's
alias key is `project:<project-name>:<absolute-checkout>` in both generated
Compose preparation and gateway registration; a shared service uses
`shared:<project-name>:<absolute-checkout>`. The alias is allocated at `up`,
released by `down` and retirement, and reclaimed by an allocation whose recorded
checkout path is definitely missing. Domain ownership is separate: a route is
the whole claim on a domain, and a dead route's domain is taken over on the next
route write.
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
leaves the durable record. Bootstrap has no teardown since v0.15 (Q-009);
environments own reset and retirement, so worker scopes are retired exactly as
above. The intended Paseo hook is
`paseo.json` `worktree.teardown`; Queue `workspace.archive` does not call
retire itself. A consumer that still calls the retired `bootstrap teardown`,
checks only running containers, or returns 0 on residue needs its own change.
This repository's own `paseo.json` hooks call `scripts/worktree-lifecycle.sh`
instead of an installed `effigy`. `setup` discards any staged binary, builds
`effigy` from the checkout with `CARGO_TARGET_DIR` pinned to
`target/worktree-hooks`, stages it at `.local-install/worktree-hooks/effigy`,
then runs the skill `link`. `teardown`
runs `container retire --yes` and then skill `unlink` with the staged binary,
so Queue milestone cleanup exercises the lifecycle code under test. Setup
stages the binary even if later QA fails. A missing or failed build, link or
retire exits non-zero; teardown never falls back to the PATH `effigy` and does
not unlink after a retire failure. Limits: the helper compiles with raw
`cargo` as bootstrap, does not install anything globally, and a checkout whose
setup never ran cannot be torn down by this hook. Covered by
`test:worktree:lifecycle-hooks`. `NORTHSTAR_SKILL_PATH` still overrides the
skill location.

Repo-owned Compose is classified as such: cleanup uses the scoped project
label, not a name prefix, and named volumes in those projects stay unless
labelled `com.effigy.persist=false`. `share_runtime_identity = true` skips
stack deletion. A TLS certificate outlives the route that needed it: nothing
removes one on a stop or a retirement, and the gateway daemon prunes the
certificates no route names once they are seven days old.

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
- holding the environment for the command through a session run (contract 053)
- for scoped worktrees, proving a fresh host checkout marker is readable at
  the container working directory and a container write reaches the selected
  checkout before dispatch
- refreshing changed Cargo input mtimes inside that mounted checkout before a
  container-routed check, so host edits invalidate stale container artifacts

Surface-specific presentation may differ. The prep contract must not.

**A deleted Colima profile is recreated, never a dead end (Tom, 2026-10-24).**
"Deleting the colima should be recoverable in effigy - a user should be able to
blindly remove the profile and it be recreated on next run when needed, it
shouldn't something you have to do." Every path that needs an Effigy-managed
Colima profile (`effigy` for stacks, `effigy-release` for the Linux workspace
artifact builder, and any other profile Effigy names) creates it when absent,
with Effigy's managed start arguments (`--ssh-agent`, DNS fallbacks, managed
resources, platform overrides), and starts it when stopped. Owned containers
that a deleted or recreated profile left stale are reconciled on the next run,
without `down` or manual cleanup. Deletion loses only that profile's data
volumes and images; the user sees which, in one line.

Runtime prep must consume captured context facts and typed execution policy. It
must not rediscover invocation cwd or handoff state after request construction.

Colima workspace-heavy memory, swap and disk recommendations are advisory and
appear only when the selected render proves a workspace binding to a service
present in the environment. An unbound catalog service, default workspace
identity or unknown direct Compose does not establish that proof. Other Colima
warnings, including default-profile sharing, remain independent.

## Cold container launch readiness

For Colima-backed `effigy container up`, Compose must not start until the
runtime is usable. A running VM is not enough: both a profile-scoped
`nerdctl info` and `buildctl debug workers` inside that profile must succeed.
A warm profile returns after those probes. A running profile that stays
unavailable through the bounded grace period gets one bounded start of the
`containerd` and `buildkit` units inside Effigy's own profile, then the same
probes again. Unit start and the re-probe are both bounded. If the runtime is
still not ready, Effigy refuses before Compose: the failure names the profile,
both readiness observations and the explicit
`effigy container profile repair` and `effigy container profile reset-runtime`
commands. Nothing stops or restarts the VM automatically — neither a task
bring-up nor a failed apply, and no failure text (an offline registry lookup
included) triggers one — because that would take down every stack on the
profile. A detected problem prints the manual command instead. Other Colima
profiles are never touched.
Explicit profile repair and reset first request a graceful stop. Their only
direct escalation reads the selected Lima instance's `ha.pid` and `vz.pid`,
checks the exact host-agent arguments and process start against the pidfile's
write time, and rechecks the evidence before signalling. Missing or changed
evidence is a refusal. External shell and SSH clients are not signalled.
When a residual host agent requires escalation on a QEMU profile with
`qemu.pid` instead of `vz.pid`, Effigy reports the missing `vz.pid` and does
not signal the QEMU driver.
Effigy does not invoke Colima's force-stop route, whose PID-file handling can
signal a reused PID; the operator may choose it after verifying ownership.
A newly started profile also waits for both probes. Failures identify the
runtime stage and include the last probe result.

After Compose up, Effigy waits before registering gateway routes until the
runtime reports the matching project and declared service publishing each
selected port. The same wait covers declared TCP service aliases. Stopped
runtime rows are inspected on timeout so an exited service is named with its
exit status. The existing project, service, published-port and host-listener
ownership checks still gate every route claim. No route is claimed from a
fallback port while the declared runtime binding is absent.

## Host load and profile VM maintenance

Every bounded Colima/nerdctl call keeps its own bound. A timeout on a loaded
host is evidence about the host, not about the service, and the contract
distinguishes the two:

- Before a bounded observation is retried, Effigy reads the host's own load
  (`/proc/loadavg`, or `sysctl vm.loadavg` on macOS). A host at or above one
  runnable task per CPU is saturated. The observation is read-only and never
  starts a container, VM or runtime command.
- An owned inspect that times out on a saturated host is retried at most twice
  with the same bound before the failure is reported. The failure names the
  observed load and states that the container's state is
  unproven and that Effigy did not start, stop or recreate it.
- A readiness refusal that ran out on a saturated host names the host in the
  same error. Without an overload observation the error keeps its
  service-language and its existing meaning.
- A service observed `Exited (141)` is reported as SIGPIPE (a closed
  stdout/stderr pipe whose reader went away), not as a health verdict. Detached
  `container up` keeps backend output in a retained file, so Effigy owns no
  reader lifetime there; see
  [Detached activation and exit-141 writer ownership](#detached-activation-and-exit-141-writer-ownership).

### Detached activation and exit-141 writer ownership

`Exited (141)` names a SIGPIPE death in the container's own leader: a write to
a pipe whose read end was closed. It does not name the reader. The private
fault controls for effigy#300 (selector `test:containers:exit141`) establish,
with native pipes and processes and a strict fake runtime, exactly what this
checkout owns and what it does not:

- Detached activation opens one regular file
  (`.effigy/runtime/logs/<project>.compose.log`) before spawn and gives the
  client that file on both stdout and stderr. A service writer that inherits it
  and outlives the reaped client has no reader to lose: it keeps writing after
  the client exits and never receives SIGPIPE.
- A bounded inventory or inspect capture does own a pipe. Cancelling it signals
  only the client's process group and positively reaps that direct child; it
  gives up its read end only after its post-exit drain grace. An independent
  writer that inherited the pipe and outlived the client survives the
  cancellation and then dies of SIGPIPE: 141 = 128 + 13.
- An exhausted absolute deadline never spawns the observation. A pending
  inventory fails as an unavailable observation and is never restart
  authority: no activation runs after it. A foreign checkout label or an
  inspect with no canonical identity is refused, and a live holder still
  forbids recreating a service under an active run.
- A genuine non-zero service exit is reported as a service failure; the SIGPIPE
  explanation is never manufactured for it.

What remains unproven is the identity of the reader in the original report. No
retained nerdctl log, process or pipe evidence names the writer, the pipe, or
the process whose read end closed, and this checkout's detached launch path
creates no pipe at all. The original papercut therefore stays open for that
observation; these controls do not claim it.

nerdctl 2.3.1 never deletes the exec process behind a health check, so the
profile VM accumulates one `health-check-<id>.pid` file per check under
`/run/containerd/io.containerd.runtime.v2.task/<namespace>/<container>/`.
`/run` is a finite-inode tmpfs; once it fills, every exec and container create
in the profile fails with `no space left on device`. nerdctl 2.4.1 deletes the
exec process and stops the leak. Effigy cannot upgrade a running profile's
runtime, so it prunes files matching exactly `health-check-*.pid`, directly
inside a containerd runtime-v2 task directory and older than ten minutes, on
`container up` (bounded, never failing and never speaking on success) and on
`effigy container profile prune-health-pid-files [--profile <NAME>]`, which
reports the counts before and after.

No other path under the task root is read or removed, and a younger file may
belong to a health check still in flight, so it is kept.

## Restarting a stopped service

A running Colima VM can still leave Compose containers stopped after the VM
restarts, a Colima restart or a crash. The runtime answers that, not Effigy:
once `up` has started the stack it asks the runtime for
`restart: unless-stopped` on every container whose stack declares no policy of
its own, and a declared policy (`no`, `always`, `on-failure`, …) wins. The
rendered Compose file names no restart policy, so the restart manager cannot
settle a container between Compose's create and start (effigy#383) and a re-up
finds an unchanged render and recreates nothing. Detached `up` and `dev` apply it
as soon as the stack is up; an attached `up` applies it when the attached run
ends, because Compose offers no started hook. The runtime restarts the
service; Effigy issues no
hand-start, no recovery start of an exited container by name, and no
`systemctl` command on nerdctl's health-check units. A stack the runtime has
not restarted reports the readiness failure that names the service and its own
health check, and `container status` lists stopped owned rows instead of
omitting them.

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

### Activation failure and recovery boundary

The 2026-10-14 Acowtancy report establishes a consumer observation: a scoped
bring-up can time out before route registration despite containers starting.
Sibling exit codes and a recurring transient health-check timer collision were
also reported. The proposed link between the timed-out process, log pipes and
SIGPIPE is unestablished; an exit code alone does not identify the sender or
pipe owner. Current Compose startup already requests `up -d`, and scoped
non-interactive retirement already supports `container retire --yes`.

The requested recovery follow-through must preserve explicit failure-state
reporting, exact project/service/container ownership and fresh published-port
evidence. An unavailable inventory cannot authorize a route or be treated as
an empty or ready stack. A later exposure reconciliation must preserve sibling
routes and running exec sessions rather than require whole-stack recreation.
Effigy recreates no service to restart it: the `restart: unless-stopped`
policy `up` applies through the runtime does that. Existing retirement
confirmation does not authorize agents to delete the reported live stack.

Detached publication retains the originally admitted context across probes and
requires it at the locked route save. A publication refusal does not rewrite
its marker from changed inputs. The checked interval and cooperative-source-
writer limits are owned by
[the gateway route trust contract](033-gateway-route-table-trust-contract.md#detached-admission-at-publication).
Detached JSON also reports `activation_stage` from a closed vocabulary:
`preparation`, `reconciliation_inventory`, `compose_activation`,
`admission_record`, `readiness`,
`runtime_integrations`, or `persistent_owner`. This identifies the checked
caller boundary without exposing backend output, argv or environment. It is
not a cause attribution, an absent-resource verdict, or readiness proof.
While that stage is `preparation`, `preparation_step` identifies the checked
boundary: `declaration`, `policy`, `admission`, `validation`, `runtime`, `mounts`,
`inventory`, `refresh`, or `dispatch`; it is null outside preparation.
`failure_kind` is one of `endpoint_observation_unknown`, `endpoint_store_refusal`,
`invocation_refusal`, `managed_readiness`, `gateway_route_refusal`, or `other`;
`identity` names the requested environment and checkout. These closed codes do
not turn an unavailable observation into absence or stop authority. `cause`
carries the underlying refusal itself — for example the gateway claim or
admission refusal that stopped the run — with secret-shaped `key=value` values
redacted and the text bounded. It is the actionable cause the earlier envelope
discarded, not backend output: no environment, argv or unredacted secret value
is retained.

Restore that admitted context before supported reconciliation; unavailable
context cannot authorize recreation, cleanup or guessed exposure.

Effigy does not recreate a stopped service to repair it: the `restart:
unless-stopped` policy `up` applies through the runtime is the restart path, and
`container retire --yes`
removes owned mutable volumes only when the operator asks. Retained file
markers establish only fixture non-mutation, not real engine volume preservation
or live exec/session continuity.

Source and controlled private-process proofs may establish the generic
mechanism. They do not retrospectively explain the reported exits or qualify
an unobserved real Linux backend. Live profile repair, held-resource cleanup,
consumer proof waivers and global installation remain separate boundaries.

Runtime activation planning belongs to `effigy-runtime-plan`. The runner
runtime-prep modules are side-effect adapters for that plan: they may start
the runtime, perform readiness checks, reconcile aliases/routes, and hold the
environment, but they must not invent a separate activation model.

## Dead-owner collection (loopback pool and domains)

A route is the whole claim on a domain: `domain -> (checkout path, target)`.
The same checkout path always replaces its own route, so a new run, a new
generation and a container-to-listener handover need no proof. A different
checkout path takes the domain over when the recorded checkout path is
missing, or the recorded target does not accept a TCP connection within 200 ms;
the takeover is logged in one line. Otherwise the write is refused with one
message naming the other checkout and the command that stops it there
(`effigy container down`). `down`, `retire` and settle remove the checkout's
routes by path, listener routes included, whether or not a supervisor is alive,
so the domain is free immediately and a following `up` from another checkout
takes it at once. Nothing else withdraws a route: there is no timer, and a
route whose target is briefly down stays until its checkout withdraws it.

The route table is the whole record of loopback ownership: an address is in
use exactly when some route references it, as its `dns_ip` or as its target. A
checkout's aliased routes share the address they already answer with, or the
lowest pool address no route references, and the route write decides it. `down`
and retirement free it by removing those routes, so there is no separate
assignment to allocate, reclaim or release, and removal and its claims commit
together. Every route write also drops routes whose recorded checkout path is
gone, for every domain, so a deleted worktree keeps neither a domain nor an
address. There is no inventory probe, no release plan and no reuse quarantine: a
released address is immediately allocatable, and the gateway answers DNS with a
one-second TTL. Growing the pool beyond `127.1.0.50` is not part of this
contract; the `lo0` aliases come from `gateway setup`.

### Detached activation failures

A detached `container up` that fails before or during admission reports the
underlying refusal in both text and JSON. The JSON failure keeps the sanitized
cause, the activation stage, the preparation step when the stage is preparation,
and the environment/checkout identity, alongside its unavailable state, failure
kind and recovery advice. Secret-shaped `key=value` details in the cause
are redacted and the cause is bounded; the generic reconciliation advice never
replaces the actionable cause.

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

Host ports are assigned by Compose or the OS. A generated service publishes
`<publish-address>::<container-port>`, so Compose picks the host port when the
service starts and Effigy reads it back from the running container when it
registers the route. Nothing is reserved, recorded or released, and no port is
"owned" across runs.

A manifest may pin a fixed host port in `[containers.<NAME>.host].ports`. The
pin is rendered as written; `up` refuses one another process already holds,
naming the holder. A runtime-scoped checkout takes an OS-assigned port for that
binding instead of competing for the primary's pin.

`[containers.<NAME>.host].stable_ports = true` opts into a stable port: one
derived from the checkout path and the published container port, and never
stored. Deriving reads nothing but those inputs, so an unchanged stack renders
the same port every time and a re-up recreates nothing; if the derived port is
taken, the runtime's bind failure names it. A shared backing service uses the
same derivation because the environment variables an app render injects need
its port before the app starts.

Prepare allocates no port and no loopback alias: a generated environment's
alias address belongs to the routes registered at `up`.

### Bind-source preparation

Policy assembly and validation resolve declared media paths and the shared
Composer cache mount without creating their directories. Activation calls
`policy::mounts::prepare_container_mount_sources` before Compose uses them,
including detached admission and task preparation. Media sources are revalidated
against the checkout at Prepare; other host mounts remain user-owned.

### Loopback alias addresses

Policy resolution reads the route table without writing it. An aliased route's
address is the checkout's recorded one, or the lowest pool address no route
references, and the registration transaction decides it; shared services and
environments without published ports use the same checkout address.

Nothing is allocated before rendering and nothing lives until `down`: the
checkout's routes are the record, they are written after the environment starts,
and `down` or retirement frees the address by removing them. No manifest change
is required for existing consumers.

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

This is not held task activation.

### Non-shell task activation ownership

This covers:

- standard routed tasks with `run_in = "container"`
- deferred requests with `[defer].run_in = "container"`
- bootstrap `run` steps that dispatch into a container without opening a shell
- Rhai `exec::run(...)` requests with container runtime policy

Contract:

- the runtime-prep phase runs before user command dispatch
- the command holds its environment through a session run for as long as it
  uses it ([053](053-environment-reconciler-contract.md) §4)
- a stack the activation starts is held: it stops when the last run using it
  ends (holder-based idle stop, no timer); `EFFIGY_DISABLE_HOST_CONTAINER_LEASE=1`
  makes task-started stacks persistent instead
- a stack that an explicit `effigy container up` left running is persistent:
  task activation borrows it and never stops it, and only an explicit teardown
  ends that ownership - every `effigy container down` form that stops the stack
  (`--global` and cwd-scoped included)
- holding must not depend on whether the request came from deferral or
  explicit task routing

This is the required shared contract for warm non-shell container reuse.
Task-shaped requests should reach this contract through
`TaskExecutionRequestBuilder` and a resolved execution plan.

## Catalog sources in workspace guests

Catalog resolution is host authority (Q-005). Workspace preparation installs
the Effigy binary only; it copies no catalog content into the guest and keeps
no guest catalog state. A guest Effigy resolves services from the same
checkout-local sources the host does: the mounted project catalog, and the
bundle and `[catalog.sources]` snapshots pinned in `.effigy/sources.lock.json`.
Only a host `effigy bundle sync` moves those pins; inside a workspace it
refuses. The v0.14 selected-pack handoff (`service pack status` and
`service pack install --path --prepare-content` inside the guest) is retired
with the catalog-pack runtime, and existing guest pack stores are ignored.

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

A linked worktree or marked ephemeral clone is included in the guarantee. Its
container targets resolve both the canonical declared alias
(`postgres.<apex>`, the value application configuration carries) and the
scope's effective alias. The canonical name is
attached to the backing service on that scope's own Compose project network,
next to the scoped name, so it can only ever name this checkout's backing
service. Compose projects, networks and volumes stay per scope; the
host-visible HTTP route keeps the scoped apex and application configuration is
never rewritten. An explicit route for the canonical name keeps its precedence
over the generated alias.

Host-native execution in a scoped checkout needs the same resolution without
machine-wide DNS and without another scope's route. A host process cannot be
given a Compose network alias, and the gateway answers one route per domain, so
the declared name stays with the checkout that published it. Effigy therefore
resolves the canonical declared alias in the environment it delivers to a host
run in a scoped checkout:

- a task's own `env`
- an env-schema value, plain or `@sensitive`, including one read from `.env`
- a declared secret env value
- a sequence step's environment, including values chained from earlier steps

Each is delivered with this checkout's effective alias in place of the
declared one. That env map is the one Effigy already owns: a consumer whose
configuration carries `postgres.<apex>` reaches its own database with no
manifest change, because the delivered value wins over the app's own `.env`
read as it already does for an env-schema value. Value source precedence is
decided first and is unchanged — a task's `env` still beats an env-schema
value, which still beats a `.env` value or a schema default — and the winning
value is then resolved. Only a whole-hostname occurrence is replaced, so
`postgres.<apex>.example.com` and an unrelated value keep their exact bytes.

Before the child starts, Effigy proves the alias: the record for the effective
domain must belong to this checkout's path and must carry a published upstream
port. A missing route, a route another checkout owns, or a route with no
published port refuses the run with the declared alias, the backing service and
`effigy container up <NAME>`, and the task's command never runs. A primary
checkout keeps its declared name and resolves nothing, and a container-bound
task is untouched: its target's own network already answers both names.

A managed TUI session (`mode = "tui"`) renders its process plan before this
boundary, so its session processes are not covered by this resolution yet; a
record for them is the remaining step.

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

- one non-shell caller path must not hold its environment while another caller path
  tears the runtime down immediately
- one caller path must not reconcile gateway/public routes while another
  caller path silently skips them for the same container policy

## Failure semantics

If runtime preparation cannot establish the contract required for the selected
execution target, Effigy should fail before dispatching the user command.

Examples:

- the target service never reports Running with a healthy status
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

Artifact-recovery and reverse-handover fixtures isolate `HOME`; compiled
children also use the fixture cwd. Run identity and admission follow
[contract 049](049-heavy-validation-admission-contract.md).
Bounded-exec public children snapshot inherited handoff markers under the
same test environment lock used by their mutators.
`test:environment:ci` covers these fixtures, DNS and Compose generation,
mkcert mounts, and health-check recovery in isolated and shared test processes.
The environment snapshot proof checks the child-spawn boundary directly.
Artifact identity and legacy-namespace matrices give each case its own test
and per-test timeout. Semantic public-path fixtures allow kernel/process startup
within their absolute operation budget; deadline-refusal cases still require
refusal at the original deadline and preserve all byte/phase assertions.

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
  container identity, and hold policy across `effigy exec`, workspace, and
  managed surfaces
- one primary checkout and two marked linked scopes, proving each host run
  receives only its own scope's declared backing-service alias, that an absent
  backing service refuses before the command runs, and that a primary checkout
  and a container-bound run keep the declared name

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
- which host runs receive scoped declared-alias resolution, and what proves a
  scope's backing service before such a run starts

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

Retired — 2026-10-28 (v0.15, effigy#424). Tom ruled MinIO unsupported and both
Acowtancy and the upstream Underlay bundle use Silo. Effigy removed the
built-in `minio` catalog service: `effigy service list` no longer offers it and
a manifest whose `catalog = "minio"` would fall back to it is refused with a
message naming `silo`, never remapped. Q-005 precedence still applies: a
project, bundle or declared source that defines its own `minio` fragment
resolves from that source, and list and render name it. Silo keeps the `MINIO_ROOT_*` environment names
and `minioadmin` default credentials as its own compatibility surface, not as a
MinIO service. Existing MinIO volumes and data are not migrated, deleted or
otherwise touched, Convergence is neither a migration target nor edited, and no
new pack release follows. Everything below is the earlier record and remains
historical.

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
Production observes exit with the kernel's own non-reaping child peek and
inventories group members through a bounded process-table capture; tests inject
only negative outcomes and use real owned children for positive completion.

Only positive root-exit and whole-group settlement evidence followed by the
actual owned wait permits reaping and a successful exit status. Persistent
observation uncertainty leaves the generation held. An actual wait failure
remains fatal and held; it is not retried as observation unavailability. A signal
permission refusal remains a refusal unless positive settlement and actual wait
resolve the race. Because macOS answers a group signal with `EPERM` when every
remaining member is a zombie (papercut 91d3fd91), a refusal from a group this
spawn path created is decided by a fresh live-member inventory: no live member
settles the group, while a live member that refused and an unreadable inventory
both keep the refusal. A zombie member holds no workload, so it keeps no group
open. Process IDs and status observations grant no signal authority, and no
signal is sent after reaping.

The retained macOS CI failure proves an observation deadline was reported; its
exact historical interleaving remains unknown. This correction does not change
unsafe/FFI blocks, doctor deadlines, admission, container cancellation or public
APIs. Existing unsafe proof-comment observations remain report-only in the
scoped Rust audit, without a soundness or whole-product certification claim.


### Exit observation evidence and published dependent replacement

The owned child's exit is no longer sampled from `/bin/ps`. `waitid` with
`WNOHANG | WNOWAIT` reports the retained child's exit without reaping it, so the
Child keeps pinning the PID and group and only the actual owned wait supplies a
status (papercut 24e9c4ab). The observation is retained once the kernel reports
the exit: macOS consumes the wait state for a repeated non-reaping peek and an
exit is monotone. `ECHILD` after the owned wait, and an expired deadline, remain
unavailable observations, never clean ones. The earlier `ps`-based typed
categories, its 500 ms per-capture cap, its 64/128-byte limits and its SHA-256
diagnostics are retired with the sampler; no parser fallback or absent-PID
inference authorizes reaping or signals.

Group membership still comes from a finite bounded `ps -Ao pid=,pgid=,stat=`
inventory, which retains only member PIDs and whether each holds workload.
Captured output, arguments, environment and credentials are excluded.

The controlled public proof publishes both a dependency and its dependent,
replaces the dependency's child generation in the published phase, then requires
a new dependent generation with the current dependency assignments after actual
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

## nerdctl health-check rendering — 2026-10-24

`nerdctl compose` does not support the Compose `CMD` exec form of a health
check. It logs that it converts the form and joins the argv with single spaces,
unquoted, into one `--health-cmd` for `/bin/sh -c`, so an argument holding a
space or a shell metacharacter changes the command: the dbgate and phpMyAdmin
probes exited `sh: 1: Syntax error: "(" unexpected` on every interval, and
every stack containing one stayed unhealthy (effigy#324).

Effigy therefore renders every `services.<name>.healthcheck.test` exec form for
the nerdctl backend as `["CMD-SHELL", <each argument quoted for /bin/sh>]`. The
rule lives in the render (`policy::render::healthcheck`), so it covers catalog
fragments, extracted fragments and project fragments in the one immutable
generation. `CMD-SHELL`, `NONE` and bare-string checks are left as written, and
a Docker render keeps the exec form Docker honours.

A stack whose checks use the exec form gets a new nerdctl render digest. That
is expected: the next `container up` publishes the new generation and Compose
recreates the containers whose configuration changed (generated stacks already
pass `--force-recreate`), so the corrected check replaces the broken one.

A catalog check that needs an argument with a space or a metacharacter — a
`node -e` or `php -r` probe — writes shell form with its own quoting, so it is
exact on any backend instead of depending on the render's conversion.
