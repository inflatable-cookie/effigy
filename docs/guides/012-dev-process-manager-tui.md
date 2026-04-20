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
```

- `effigy dev` is not a global built-in command.
- It is the repo's own managed task, usually `[tasks.dev]`.
- `effigy dev <profile>` selects a repo-owned profile under
  `[tasks.dev.profiles.<name>]`.
- On interactive terminals, Effigy launches the managed session runtime.
- On non-interactive terminals, Effigy renders a managed plan summary or stream
  output depending on task/runtime state.

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

[tasks.dev.managed]
container_lifecycle = true
health_wait = true
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

The shipped managed-task additions live under `[tasks.<name>.managed]`.

Current bounded fields:

- `container_lifecycle = true`
  - lets one managed task own the resolved workspace-backed container lifecycle
- `health_wait = true`
  - waits on the task-owned container health path before projecting readiness
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

Run-array note:

- if another task sequence uses `{ task = "dev" }` to reference a
  managed/concurrent task, Effigy delegates through a nested `effigy <task>`
  invocation rather than requiring that managed task to also define an inline
  `run = ...`

Optional global shell command override:

```toml
[shell]
run = "exec ${SHELL:-/bin/zsh} -i"
```

If omitted, Effigy uses:

- `exec ${SHELL:-/bin/zsh} -i`

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

[tasks.dev.managed]
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

- One tab per managed process or role.
- If a process has `shutdown_on_exit = true`, its exit becomes a full-session
  stop signal for the rest of the managed stack.
- `role = "lifecycle"` owns the task's resolved workspace container bring-up
  and shutdown.
- `role = "shell"` uses the primary-service container shell and coexists with
  the lifecycle-owned container session.
- `managed.health_wait = true` delays the final ready projection until the
  task-owned container health gate succeeds.
- `managed.ready_message` is shown only after that ready state is reached.
- `managed.gateway = true` starts the gateway before the managed runtime when
  the task-owned container environment declares local DNS ownership.
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

For repos using the fuller shipped local-dev story, `effigy dev` sits at the
end of this chain:

1. `effigy service` for bundled service-fragment inspection or extraction
2. `effigy container` for environment bring-up and data lifecycle
3. `effigy gateway` for local domains and TLS-backed routes
4. `effigy exec ...` for one ad-hoc command in the dev container
5. repo-owned managed task such as `effigy dev` when the repo wants all of
   that under one named session front door

That means `effigy dev` should normally be the repo's opinionated aggregator,
not the only way the substrate can be used.

## 8) Environment Controls

- `EFFIGY_MANAGED_STREAM=1`
  - bypasses TUI and runs the selected profile in stream mode
- `EFFIGY_MANAGED_TUI=0|false`
  - disables TUI auto-launch and renders managed plan output
- `EFFIGY_MANAGED_TUI=1|true`
  - forces TUI launch
- `EFFIGY_TUI_DIAGNOSTICS=1|true`
  - enables post-run TUI diagnostics summary for emulator/runtime debugging

## 9) Validation Checklist

1. Run `effigy dev` from repo root and verify the expected default profile
   opens.
2. If the repo uses a workspace-backed container binding, verify the resolved
   environment starts and shuts down with the managed session.
3. If the repo uses `managed.gateway = true`, verify the gateway starts only
   when the container declares the expected local domain route.
4. If the repo uses `managed.health_wait = true`, verify readiness is delayed
   until the health gate passes.
5. If the repo uses `managed.ready_message`, verify the final ready message is
   honest and repo-specific.
6. If the repo uses `role = "shell"`, verify shell input capture and completion
   work in the embedded terminal tab.
7. If the repo uses `shutdown_on_exit`, close the designated root process and
   verify Effigy tears down the remaining child processes automatically.

## Related Guides

- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)

## Next Step

After choosing whether your repo needs the older concurrent-only shape or the
full managed dev-front-door contract, codify that task in `effigy.toml` and
verify the full local session with `effigy dev` before adding more onboarding
prose or wrapper scripts.
