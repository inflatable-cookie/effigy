# 012 - Dev Front Door And Managed Sessions

Use this guide when a repo wants one honest `effigy dev` front door on top of
the shipped managed runtime, container surface, gateway, and ad-hoc exec path.

This is no longer just a TUI guide. The shipped `v0.3` model is a repo-owned
managed task contract that can compose:

- ordinary concurrent tasks and processes
- one lifecycle-owned container environment
- one embedded shell tab
- one readiness gate and ready message
- one gateway auto-start path when the container declares local domains

## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`, `ADOPT`
- Target movement: multi-command local bring-up turns into one repo-owned dev
  task instead of wrapper-script glue and compose muscle memory.

## 1) Invocation

The default path is still repo-owned task dispatch:

```bash
effigy dev
effigy dev <profile>
effigy dev --headless
effigy dev --detach
effigy dev status
effigy dev logs [process]
effigy dev stop
```

- `effigy dev` is not a global built-in command.
- It is the repo's own managed task, usually `[tasks.dev]`.
- `effigy dev <profile>` selects a repo-owned profile under
  `[tasks.dev.profiles.<name>]`.
- On interactive terminals, Effigy launches the managed session runtime.
- `--headless` follows the same run as text instead of a terminal UI.
  `EFFIGY_MANAGED_HEADLESS=1` selects the same mode.
- `--detach` starts the run in the background and returns once every
  workload has started, with the run id and its `status`, `logs` and `stop`
  commands.
- On non-interactive terminals, Effigy renders a managed plan summary or stream
  output depending on task/runtime state.

Managed Cargo/Oxide producers use the initiating ANSI pane capability and
respect `NO_COLOR`, CI and explicit colour preferences. Their canonical logs
contain sanitized, redacted plain text; panes receive optional typed styles
from the same writer result. A missing or expired style window renders plain
text, and plain/JSON readers do not interpret terminal controls.

On the first host-process start after upgrading from the raw-log writer,
Effigy settles the prior owner, retains its old log in private
`.legacy-host-logs` quarantine, and creates a fresh inode at the same public
log path. **Stop and reopen pre-upgrade attached log followers after upgrading.**
An already-open POSIX descriptor cannot be revoked by that rename. The bundled
Underlay follower detects the new inode and reports a log-restarted gap,
including when the replacement grew beyond the old byte offset. Old bytes
are not replayed, and continuity across this one-time transition is not
promised. Quarantined data is retained for explicit operator handling.

## 2) Two Valid Shapes

There are now two honest ways to use managed sessions.

### A. Concurrent TUI Without Container Ownership

This is the older and still valid shape when the repo only wants one managed
multi-process session:

```toml
[tasks.dev]
mode = "tui"

concurrent = [
  { task = "catalog-a/api", start = 1, tab = 3 },
  { task = "catalog-a/jobs", start = 2, tab = 4, start_after_ms = 1200 },
  { task = "catalog-b/dev", start = 3, tab = 2, shutdown_on_exit = true },
  { run = "my-other-arbitrary-process", start = 4, tab = 1 },
  { task = "shell", start = 5, tab = 5 }
]
```

Use this when the repo already owns startup elsewhere and only wants managed
tabs, process ownership, and shell access.

### B. Repo-Owned Dev Front Door With Managed Metadata

This is the fuller shipped `v0.3` shape when the repo wants one named task to
own the local environment:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
workdir = "."

[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
health_wait = true
health_wait_timeout_secs = 90
ready_message = "App ready at http://project.test"
gateway = true

concurrent = [
  { name = "app", role = "lifecycle", start = 1, tab = 1 },
  { name = "terminal", role = "shell", start = 2, tab = 2 },
  { task = "catalog-a/jobs", start = 3, tab = 3 }
]
```

Use this when the repo wants `effigy dev` to feel like one daily-driver entry
point instead of a sequence of `service`, `container`, `gateway`, `exec`, and
ad-hoc shell steps.

## 3) Managed Contract

The shipped managed-task additions live directly under `[tasks.<name>]`.

### The environment is the session

`effigy dev` is the session in both placements. A managed task bound to a
container environment (`workspace = "app"` resolving to a
`[containers.<name>]` environment) owns that environment, and its tabs are the
environment's own:

- the run's own output tab (named `lifecycle` by default; it carries the
  `[environment]` readiness lines, starts and exits),
- one tab per `[[containers.<env>.host_processes]]` entry, in declaration
  order, with a status line (`starting`, `running`, `ready on <url>`,
  `restarting`, `exited <code>`) and that process's live log,
- one tab per declared workload (`concurrent` entries or the task's own
  `run`), and a local shell tab when the profile declares one.

Placement is the environment's property, not the selector's. Whether a
workload `run_in = "host"` or execs into the environment's primary service,
the session brings the environment up the same way, shows the same host
process tabs, and closes the same way. A bundle therefore needs no `*:tui`
selector and no wrapper task: `effigy dev` in hybrid and container mode is one
session.

A plain task needs no `concurrent` entry per process:

```toml
[tasks.dev]
mode = "tui"
workspace = "app"
run = "bun run dev"          # optional: the one workload of the session
```

That session owns the environment because it declares no entries of its own.
A task that does list `concurrent` entries keeps the older shape unless it
says `container_lifecycle = true` (then the environment is owned, and the
entries add tabs), or `container_lifecycle = false` (the environment is
whoever else started it). `role = "lifecycle"` keeps working, and is no longer
required for `gateway`, `health_wait` or a plain bound session.

Readiness orders the session: workloads start after the environment's `up`
finished and its primary service is running and healthy. Declare that dependency with
`after = ["environment"]` when you want it written down, and order an
environment's host processes with their own `depends_on`.

Current bounded fields:

- `container_lifecycle = true`
  - lets one managed task own the resolved workspace-backed container lifecycle
- `health_wait = true`
  - waits on container-owned routes started by the lifecycle entry before
    projecting readiness; routes targeting external host processes are not
    probed at this stage
- `health_wait_timeout_secs = 90`
  - sets the readiness deadline; defaults to 60 seconds and requires
    `health_wait = true`
- `ready_message = "..."`
  - projects one repo-owned ready message after `health_wait` succeeds
  - requires `health_wait = true`
- `gateway = true`
  - starts the shipped gateway path before the managed runtime starts
  - requires a workspace-backed container binding on the task
  - requires one `concurrent` entry with `role = "lifecycle"`
  - requires the resolved container to declare local DNS ownership

Current special concurrent roles:

- `role = "lifecycle"`
  - the lifecycle-owned process that starts the task's named container session
- `role = "shell"`
  - opens the primary-service container shell through the shipped
    `effigy container shell` path

The important boundary is that this remains repo-owned task configuration. The
guide is describing one richer task contract, not a special-case built-in dev
command.

## 4) Concurrent Entry Options

Profile entries support:

- direct task references (`catalog/task`) via `task = "..."`, or
- arbitrary process commands via `run = "..."`, or
- relative path task references (`../repo/task`) via `task = "..."`, resolved
  from the current catalog root
- integrated shell access through either:
  - legacy `task = "shell"` for the older concurrent-session shape
  - `role = "shell"` for the managed dev-front-door shape
- optional lifecycle root via `shutdown_on_exit = true` on one process or a
  small set of processes when their exit should stop the whole managed session
- optional profile overrides via `[tasks.dev.profiles.<name>]` with their own
  `concurrent = [...]`
- `start = <N>` controls spawn order. Equal ranks keep manifest order.
- `after = ["environment"]` states the ordering dependency the session already
  has: the environment phase finishes before any workload starts. It is the
  one accepted value; order environment host processes with `depends_on`.
- `start_after_ms` still delays its entry for the v0.15 line, but it no longer
  orders a managed session. The manifest's one deprecation notice names the
  replacement, and the key is removed after v0.15
  (`docs/knowledge/retired.toml`, `managed-start-after-ordering`).

Run-array note:

- if another task sequence uses `{ task = "dev" }` to reference a
  managed/concurrent task, Effigy delegates through a nested `effigy <task>`
  invocation rather than requiring that managed task to also define an inline
  `run = ...`

Managed shell tabs always use:

- `exec ${SHELL:-/bin/zsh} -i`

`[shell]` was removed in v0.15: there is no manifest override. A manifest that
still declares it loads, and the table is ignored with one deprecation notice
per manifest.

## 5) Profile Example

Profile overrides still work on the fuller managed contract:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
workdir = "."

[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
health_wait = true
ready_message = "App ready at http://project.test"
gateway = true

concurrent = [
  { name = "app", role = "lifecycle", start = 1, tab = 1 },
  { name = "terminal", role = "shell", start = 2, tab = 2 },
  { task = "catalog-a/jobs", start = 3, tab = 3 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { name = "app", role = "lifecycle", start = 1, tab = 1 },
  { name = "terminal", role = "shell", start = 2, tab = 2 },
  { task = "catalog-a/jobs", start = 3, tab = 3 },
  { task = "catalog-c/admin", start = 4, tab = 4 }
]
```

## 6) Runtime Behavior

- One tab per managed process or role, and one per environment host process
  when the session owns its environment. Host process tabs show the status the
  process's own supervisor reports and follow its log; the status line is
  `starting`, `running`, `ready on <url>` (a declared listener that published
  its route), `restarting` (the restart policy replaces the exited generation)
  or `exited <code>`.
- `--headless` shows the same facts as text: `[name] [host] <status>` and
  `[name] <line>` for each environment host process, beside each workload's
  `[name] <line>`.
- If a process has `shutdown_on_exit = true`, its exit becomes a full-session
  stop signal for the rest of the managed stack.
- `role = "lifecycle"` owns the task's resolved workspace container bring-up
  and shutdown.
- `role = "shell"` uses the primary-service container shell and coexists with
  the lifecycle-owned container session.
- `health_wait = true` delays the final ready projection until the
  task-owned container health gate succeeds.
- `ready_message` is shown only after that ready state is reached.
- `gateway = true` starts the gateway before the managed runtime when
  the task-owned container environment declares local DNS ownership.
- The interactive TUI renders its first frame without waiting for configured
  startup delays to finish; zero-delay children may start concurrently with
  that first paint. Tabs show waiting, starting, running, and failed startup
  states; child output and key input continue to be handled while a configured
  `start_after_ms` delay is pending.
- Child startup still follows the configured order, and each
  `start_after_ms` remains a delay before that entry is spawned. `Ctrl+C`
  cancels pending startup and gracefully stops only children already owned by
  the session. A child startup failure is shown in its tab and stops the
  already-started session children before terminal restoration.
- Non-shell tabs use input panel mode (`Tab` toggles command/insert; `Enter`
  sends input).
- Shell tab uses direct terminal capture mode:
  - `Ctrl+G` toggles shell capture on/off
  - when capture is on, keypresses go directly to shell, including `Tab`
    completion
  - shell tab label shows `shell [live]` when capture is active
- `Tab` / `Shift+Tab` cycles tabs.
- `q` or `Ctrl+C` exits and terminates child processes.

## 7) Local Dev Relationship

For repos using the fuller bundle-backed local-dev story, `effigy dev` sits at the
end of this chain:

1. `effigy bundle inspect` / `effigy bundle sync` to inspect or refresh the
   active bundle source the repo already declares
2. `effigy service list` / `effigy service extract` for bundled service
   fragments when the repo needs to take ownership of one
3. `effigy container up` / `effigy container status` for substrate lifecycle;
   without a name it resolves the `[systems].default` workspace container, and
   `--system <NAME>` selects one declared system
4. `effigy gateway up` / `effigy gateway status` for local domains and
   TLS-backed routes (usually auto-started via `gateway = true` on the
   managed task)
5. `effigy workspace` for an interactive Linux-native maintenance shell
   inside the resolved workspace service
6. `effigy exec ...` for one ad-hoc command inside the dev container
7. repo-owned managed task such as `effigy dev` when the repo wants all of
   that under one named session front door

That means `effigy dev` should normally be the repo's opinionated aggregator,
not the only way the substrate can be used. Use `effigy container` + `effigy
workspace` when you want substrate access without launching the whole app
concurrent; use `effigy dev` for daily runtime.

## 8) Environment Controls

### One supervisor per run

Every managed run has one supervisor: a detached Effigy process in its own
session that owns the run. It admits the run in the kernel journal, then runs
setup, the gateway, the environment and every workload, and its one settle is
the only cleanup. The TUI, `--headless` and `--detach` are front ends over that
run:

- The TUI and `--headless` stay attached. `--headless` prints the run's own
  lines (`[environment]`, `[managed]`) and every workload's output as
  `[name] line`. Closing the terminal, SIGHUP, SIGINT, SIGTERM or SIGKILL to
  that client stops and settles the run, and the client exits non-zero when
  the settle leaves residue. A second interrupt within two seconds escalates
  the stop.
- `--detach` returns once every workload has started, before `health_wait`
  routes have answered (`status` and `logs lifecycle` show them). The run
  keeps going until `effigy dev stop`. Killing a detaching client before that point stops
  the run it started.

The journal is the run's only record. `.effigy/runtime/managed/` session
files, stop markers and lifecycle state files are gone. A live run left by
v0.14 is refused with its own stop command and never adopted.

Headless runs never allocate an interactive TTY and never start shell-role
tabs: shells belong to a terminal.

The environment phase runs before any workload: `container up` as a child of
the run, then the service's own readiness, polled with backoff until the
workspace service answers or the readiness budget runs out
(`health_wait_timeout_secs`, default 60 seconds). Readiness is observed and
never a fixed timer, so a slow first engine start delays the workloads
instead of failing them, and workloads need no `start_after_ms` to wait for
the environment. With `health_wait = true`, the task's routes are probed once
every workload has started, since the workloads serve them; the ready
message prints when they answer, and a route that never answers within the
budget fails the run.

The environment's host processes belong to the run's environment, so the same
close applies to them in both placements: a run that started the environment
stops its host processes and the backend, and a run that borrowed an
environment leaves both running. `effigy <task> status` lists the run's
workloads; `effigy container status` lists the environment's host process runs,
their PIDs and their last exits.

A run holds its environment while it lives, so `container down`, handover
and recovery refuse until it settles. A run that brought its environment up
stops it when the run settles. A run that found an environment already
running from an explicit `container up` borrows it and leaves it running. A run that holds an environment stays up
when its workloads exit, until a stop; `shutdown_on_exit` still ends it.

A stop reports settled only after the run's settle finished:
- every owned host process has ended, including the process groups the run
  exec'd into its container environment. Those are ended through the container
  adapter after checking the marker's run nonce, so an in-container
  `cargo run` cannot outlive the stop;
- the environment the run started is down (volumes kept);
- the run is `closed` in the journal. `cleanup incomplete` lists the residue
  in `status`, `stop` exits non-zero, and the next run settles it first.

Use another shell or automation process to inspect or stop it:

```sh
effigy dev status
effigy dev status --json
effigy dev status admin
effigy dev logs
effigy dev logs api
effigy dev logs api --follow
effigy dev logs --profile admin
effigy dev logs lifecycle
effigy dev stop
effigy dev stop admin
```

`status` reports the run, its environment and each started process with PID,
state and exit code. `--json` prints `effigy.managed.status.v1`. `logs <name>`
reads a workload's log; `logs lifecycle` reads the run's own output (setup,
environment, readiness, starts and exits). A failed run names the failing
process, its log command and the log's tail in the task error. `stop` is
idempotent: on a closed run it reports the run's outcome. If the supervisor
died, the run's sentinel has already settled it, or `stop` settles it.

Start delays remain interruptible: stop cancels children that have not
started yet. While a run is live, recovery, lease expiry and another
environment's handover cannot stop its environment before that run settles;
a handover asks the run to stop and waits for its settle. Use the task's
`stop` control before an unrelated explicit `container down`.

- `EFFIGY_MANAGED_HEADLESS=1`
  - follows the run as text (`--headless`)
- `EFFIGY_MANAGED_TUI=0|false`
  - disables TUI auto-launch and renders managed plan output
- `EFFIGY_MANAGED_TUI=1|true`
  - forces TUI launch
- `EFFIGY_TUI_DIAGNOSTICS=1|true`
  - enables post-run TUI diagnostics summary for emulator/runtime debugging

### One vault question per session

When the checkout declares a built-in vault and that vault exists, a managed
session resolves vault access **once, before any front end renders** — TUI,
`--headless` or `--detach` — and everything it starts uses that access:
the environment `up`, the run's supervisor, every workload, shell and
lifecycle tab, host-process supervisors and their processes, and any nested
`effigy` run inside them. A locked vault asks on the real terminal once,
before the session takes the screen, instead of inside a tab; a nested run
never asks. Nothing is persisted: the session does not seal a command
credential, so the workaround of running `effigy secrets unlock` before the
TUI is no longer needed for this.

The order is the one secrets already use: a passphrase this invocation holds,
the sealed command credential `effigy secrets unlock` leaves, or one prompt.
With no terminal and none of those the session refuses before it starts
anything and names the fix — `effigy secrets unlock` once, or
`EFFIGY_SECRET_PASSPHRASE` for one invocation. The local-dev key is unchanged:
it serves a resolved `dev` session's own secrets without a passphrase and is
never handed to another task's children.

### Restart the managed dev session

To restart the repository's managed `dev` session, inspect and stop that run,
then start the same task in the background:

```sh
effigy dev status
effigy dev stop
effigy dev --detach
```

This sequence restarts the managed application processes and leaves the shared
gateway available to other sessions. Gateway auto-start prints an operator
notice before waiting if administrator approval may be needed. If startup times
out, run `effigy gateway status` before retrying; an unknown process identity
keeps its records held for reconciliation.

## 9) Validation Checklist

1. Run `effigy dev` from repo root and verify the expected default profile
   opens.
2. If the repo uses a workspace-backed container binding, verify the resolved
   environment starts and shuts down with the managed session.
3. If the repo uses `gateway = true`, verify the gateway starts only
  when the container declares the expected local domain route.
4. If the repo uses `health_wait = true`, verify readiness is delayed
  until the health gate passes.
5. If the repo uses `ready_message`, verify the final ready message is
  honest and repo-specific.
6. If the repo uses `role = "shell"`, verify shell input capture and completion
   work in the embedded terminal tab.
7. If the repo uses `shutdown_on_exit`, close the designated root process and
   verify Effigy tears down the remaining child processes automatically.

## Related Guides

- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](055-everyday-workflows.md)
- [`063-container-system-guide.md`](063-container-system-guide.md)
- [`064-system-workspace-and-dev-contract.md`](064-system-workspace-and-dev-contract.md)
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)

## Next Step

After choosing whether your repo needs the older concurrent-only shape or the
full managed dev-front-door contract, codify that task in `effigy.toml` and
verify the full local session with `effigy dev` before adding more onboarding
prose or wrapper scripts.
