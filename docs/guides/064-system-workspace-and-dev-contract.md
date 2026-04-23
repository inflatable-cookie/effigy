# 064 - System, Workspace, and Dev Contract

Use this guide when the question is "what does the container-backed dev UX
mean?" — what the public contract is for `system`, `workspace`, and `dev`, and
how they fit together.

This started as a contract-direction page. The contract is now shipped: the
public `effigy system ...` surface exists, `effigy workspace` opens the
resolved workspace shell, and managed `mode = "tui"` tasks (usually `dev`)
bind to systems through the manifest. The `effigy container ...` surface is
retained as the direct compose-lifecycle operator surface and coexists with
`system`; see the "Current Shipped State" section below for the division.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Target movement: Effigy keeps `dev` as the normal concurrent front door while
  giving repos one first-class system substrate surface for container-backed
  work.

## Start Here

Use this page when you want to answer any of these:

- does `dev` keep owning the app runtime?
- how does a repo bring infra up without launching the whole app stack?
- where do package installs, updates, migrations, and one-off Linux-native
  maintenance happen?
- what is the relationship between `container` and `system` as public
  commands?

Short answer:

- `dev` keeps its historic role as the repo-owned concurrent runner
- `system` is the public substrate lifecycle surface
- `shell` and `workspace` are the interactive Linux-native maintenance surface
- `container` remains as a direct compose-lifecycle surface (see "Current
  Shipped State" below)

## Current Shipped State

This contract is live in the shipped product with one nuance: `container` and
`system` coexist as public command families, with a clear division of labor:

- `effigy system ...` owns the whole substrate — VM + compose + gateway +
  workspace handoff, resolved from `[systems.<name>]`
- `effigy container ...` operates directly against the compose lifecycle for
  repos that declare `[containers.<name>]` without a surrounding system, and
  for data-lifecycle commands (`data export`, `data import`, `reset`, `eject`)
  that remain compose-scoped
- `effigy workspace` is the DX shortcut that ensures the selected system is up,
  then opens the resolved workspace shell
- managed `mode = "tui"` tasks (usually `dev`) bind to a system via
  `system = "..."` / `workspace = "..."` / `container_lifecycle = true` /
  `gateway = true` / `health_wait = true` / `ready_message = "..."`

New repos should prefer the `[systems.<name>]` shape and use
`effigy system` + `effigy workspace` as the top-level operator surface.
`[containers.<name>]` remains supported and is the right choice when the repo
has a single compose environment with no surrounding system wiring.

## 1) Problem

The current container-backed path collapses two different lifecycles into one
front door:

- system lifecycle: DB, object storage, mail, gateway, workspace container
- app lifecycle: API, jobs, Vite servers, watchers, interactive shell tabs

That is good for one-shot startup but awkward for normal development work:

- adding or updating packages
- running one-off setup or repair commands in Linux
- rebuilding dependency state without relaunching the whole app stack
- bringing the substrate up first and deciding later which app shape to run

The old non-container `effigy dev` model was simpler: it just ran the repo's
concurrent app shape. The container-backed model should preserve that meaning
instead of redefining `dev` as "the whole environment plus app runtime plus
repair logic".

## 2) Contract Summary

Effigy's public local-dev contract should split into three layers:

- `effigy system ...`
  substrate lifecycle for container-backed repos
- `effigy shell` and `effigy workspace`
  interactive access to the running Linux-native workspace
- `effigy dev`
  repo-owned concurrent app runtime, exactly as before

Core rule:

- `dev` may ensure the system exists when the task requires it
- `dev` does not become the only surface for touching the system

## 3) Public Command Surface

### 3.1 Canonical system surface

The public substrate command family is:

```sh
effigy system up
effigy system down
effigy system status
effigy system logs
```

Optional later extensions can include system-scoped data/reset helpers if they
still belong on the public product path:

```sh
effigy system reset
effigy system data ...
```

The exact subcommand set can stay small at first. The important contract point
is that `system` is the public lifecycle surface and `container` is not.

### 3.2 Interactive access surface

Two interactive entrypoints sit above `system`:

```sh
effigy shell
effigy workspace
```

Contract:

- `effigy shell` is the generic interactive shell surface for the repo's
  primary workspace service or one explicitly selected service
- `effigy workspace` is the DX shortcut for "ensure system is up, then open the
  primary workspace shell"

Both surfaces are Linux-native maintenance paths. They exist so users can do
normal development work inside the real runtime substrate without relaunching
the whole app concurrent.

### 3.3 App runtime surface

`effigy dev` keeps its existing meaning:

- resolve the repo-owned `dev` task
- run the repo-owned `concurrent` shape
- do not introduce a separate `effigy app up` concept

If a repo wants other app-runtime shapes, it defines them the same way it does
today: more tasks, more profiles, or more concurrents.

## 4) Ownership Rules

System lifecycle must be ownership-aware.

The key distinction is:

- system already running before the command started
- system started by the current command invocation

### 4.1 `effigy system up`

Contract:

- idempotent
- if the system is already up, report that cleanly and do nothing destructive
- if the system is down, bring it up and record that it is now externally
  running

### 4.2 `effigy dev`

Contract:

1. Resolve the normal repo-owned `dev` task.
2. If that task requires system substrate and the system is down, run
   `effigy system up` automatically before launching the concurrent runtime.
3. Record whether this `dev` invocation started the system itself.
4. On `dev` exit:
   - if this invocation started the system, apply the configured closeout
     policy
   - if the system was already running before `dev`, leave it alone

Default closeout policy for auto-started systems:

- `auto-stop`

Rationale:

- it keeps the one-shot `effigy dev` experience clean
- it avoids sticky infra after a quick session
- it still respects pre-existing system state by leaving it alone

Optional later policy knobs can support:

- `auto-stop`
- `leave-running`
- `prompt`

But the baseline contract should not require an exit prompt.

### 4.3 `effigy shell` / `effigy workspace`

Contract:

1. Ensure the system exists.
2. Open the interactive workspace shell.
3. Record whether this shell/workspace invocation started the system.
4. On shell exit:
   - if this invocation started the system, apply the same closeout policy
   - if the system was already running, leave it alone

This makes `workspace` act like a leased system session:

- open Linux-native maintenance environment
- do work
- exit cleanly
- tear the substrate down only when the command itself created it

## 5) Package Management and Everyday DX

This contract intentionally separates steady-state app startup from dependency
mutation.

`effigy dev` should not become the hidden package-manager repair surface.

Allowed `dev` behavior:

- validate that required substrate exists
- auto-start missing substrate when the task requires it
- perform only small, deterministic, bounded prep that is clearly part of app
  runtime startup

Disallowed default `dev` behavior:

- large implicit package installs
- hidden dependency graph mutation
- open-ended environment repair loops
- repo-specific setup heuristics that surprise the user during normal startup

The normal package-add workflow for a container-backed Vite repo should be:

```sh
effigy workspace
cd /workspace-root/<repo>/acme-front
bun add <pkg>
exit
effigy dev
```

That gives the developer:

- Linux-native install behavior
- obvious ownership of the mutation
- no hidden `dev`-time dependency churn

The same pattern covers:

- `bun update`
- migrations
- cache cleanup
- manual investigation
- one-off maintenance commands

## 6) Relationship to Manifest Contracts

The user-facing contract changes before the manifest substrate contract does.

Near-term rule:

- public command surface uses `system`
- implementation may still read existing container-backed manifest sections

That means a repo can keep current substrate declaration shapes while Effigy
renames the public lifecycle surface from `container` to `system`.

The app-runtime side stays familiar:

- `effigy dev` still runs repo-owned tasks
- managed/concurrent runtime still describes app tabs and roles

What changes is where substrate lifecycle lives:

- not on `container`
- not hidden inside `dev`
- on `system`

## 7) Manifest Naming Contract

The public manifest language should match the command language.

Use these nouns:

- `system`
  composed substrate environment
- `workspace`
  named execution space inside a system
- `container`
  one implementation building block a workspace may use

Do not use `runner` as the public manifest noun for this feature.

Reason:

- `runner` is already overloaded across Effigy
- `workspace` matches the operator mental model better
- `workspace` also lines up with `effigy workspace`

That gives one clean question ladder:

- which environment should this use? `system`
- where inside that environment should this run? `workspace`

### 7.1 Canonical manifest shape

The canonical model is:

- named systems
- named workspaces inside each system
- explicit default system
- explicit default workspace per system

Example:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "app"

[systems.dev.workspaces.node]
container = "app"
working_dir = "/workspace/frontend"

[tasks.test]
run = "cargo test"

[tasks.frontend:install]
workspace = "node"
run = "bun install"

[tasks.smoke]
system = "ci"
workspace = "app"
run = "./scripts/smoke.sh"
```

Task defaults resolve in this order:

1. task `system`
2. repo `systems.default`
3. task `workspace`
4. system `default_workspace`

If resolution fails at any step, Effigy should fail clearly instead of
guessing.

### 7.2 Task contract

For task execution, `workspace` is the natural binding knob.

Example:

```toml
[tasks.dev]
workspace = "app"
run = "npm run dev"
```

This should mean:

- run the task inside the resolved system
- use the named workspace inside that system
- do not require every task to restate the same container binding

If a task explicitly selects a host-only execution path while the active route
is system-bound, Effigy should error instead of silently escaping to host.

### 7.3 `workspace` command alignment

The command surface should resolve through the same names:

```sh
effigy workspace
effigy workspace <name>
effigy workspace <name> --system <system>
```

Contract:

- `effigy workspace` opens the default workspace for the default system
- `effigy workspace <name>` opens that workspace on the default system unless
  another system is explicitly selected
- workspace names are manifest references, not magic literals

That keeps CLI and manifest language aligned:

- users define workspaces
- tasks reference workspaces
- `effigy workspace` opens workspaces

## 8) Workspace Shortcut for Simple Repos

The canonical model stays `system -> workspace -> container wiring`, but simple
repos should not be forced to declare every layer separately.

Allow one shortcut:

- a workspace may define its backing container inline

Example:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }
```

Rules:

- inline container form is sugar
- the workspace name is already the identity; do not require a duplicate
  container `name`
- the loader must normalize the shortcut into the same canonical resolved model
  used by explicit container declarations
- runtime, validation, and docs should still reason about one underlying model

This shortcut exists for the happy path:

- one system
- one workspace
- one backing container

When a repo needs more than that, it should graduate to the full explicit form:

- shared containers
- multiple workspaces over one container
- richer service composition
- reusable named substrate pieces

### 8.1 Normalization rule

The shortcut must not create a second runtime path.

Contract:

- parse inline workspace container config
- synthesize the equivalent canonical container-backed workspace definition
- continue resolution and execution against the canonical form only

That keeps the schema easy on day one without creating long-term model drift.

### 8.2 Schema-facing examples

Implementation work should treat these as the baseline accepted shapes.

Fully explicit form:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[containers.app]
image = "node:22"
mount = "./:/workspace"

[systems.dev.workspaces.app]
container = "app"
working_dir = "/workspace"

[tasks.dev]
workspace = "app"
run = "npm run dev"
```

Shortcut form:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }

[tasks.dev]
workspace = "app"
run = "npm run dev"
```

Cross-system override:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.ci]
default_workspace = "app"

[tasks.smoke]
system = "ci"
run = "./scripts/smoke.sh"
```

In the last example, `workspace` resolves from `systems.ci.default_workspace`.

### 8.3 Resolution and validation rules

Implementation should enforce these rules directly:

- `systems.default` must reference a declared system
- `systems.<name>.default_workspace` must reference a declared workspace inside
  that same system
- `tasks.<name>.system`, when present, must reference a declared system
- `tasks.<name>.workspace`, when present, must reference a declared workspace
  inside the resolved system
- if a task resolves to a system but no workspace can be resolved, fail clearly
- if a workspace references `container = "name"`, that name must resolve to a
  declared container
- if a workspace uses inline `container = { ... }`, loader normalization must
  assign that container an internal identity derived from the workspace rather
  than requiring a user-facing duplicate `name`

Error posture:

- no fallback guessing across systems
- no fallback guessing across workspaces
- no silent host escape when the resolved task path is system-bound

### 8.4 Public naming

The public manifest contract is:

- `system`
- `workspace`

Do not keep parallel public nouns for the same execution binding. Container
selection belongs under the resolved workspace contract, not as a second
task-level field.
- canonical normalization
- one resolved runtime model

## 9) Command Family Decision

The shipped product keeps both `system` and `container` as public command
families with non-overlapping responsibilities:

- `system` owns substrate lifecycle for manifests that declare
  `[systems.<name>]` — VM/profile, compose, gateway, workspace handoff, and
  recovery (`repair`, `reset-runtime`)
- `container` owns direct compose lifecycle (`up`, `down`, `status`, `logs`,
  `shell`, `reset`, `eject`) and data lifecycle (`data list`, `data export`,
  `data import`, `data pull-production`) for manifests that declare
  `[containers.<name>]`

Rationale for the division:

- substrate responsibility is bigger than compose — `system` also owns gateway
  state and workspace handoff, which operators should be able to address
  directly without learning compose semantics
- direct compose lifecycle is still a useful operator surface for data
  export/import and for repos that do not need a surrounding system
- cross-project views (`container status --all`, `container stats --all`) are
  intentionally compose-scoped and belong on `container`

Public docs and examples should lead with `system` + `workspace` for new
container-backed repos, and should only introduce `container` directly when
the manifest uses the `[containers.<name>]` shape without a surrounding
system.

## 10) Workspace-In-Container Behavior

One subtle case needs explicit treatment:

- user enters `effigy workspace`
- user then runs `effigy dev` from inside that workspace shell

Contract:

- `dev` must detect that it is already inside the active workspace substrate
- it must not recursively try to re-own or re-bootstrap the system
- it should just run the repo-owned app concurrent against the already-running
  substrate

This keeps `workspace` useful as the maintenance/login surface without making
nested `dev` confusing.

## 11) Expected User Flows

### 11.1 One-shot daily startup

```sh
effigy dev
```

Behavior:

- auto-start system if needed
- run the repo-owned dev concurrent
- auto-stop system on exit only if this invocation started it

### 11.2 Linux-native maintenance session

```sh
effigy workspace
```

Behavior:

- ensure system exists
- open workspace shell
- user performs package or maintenance work inside Linux
- on exit, auto-stop only if this invocation started the system

### 11.3 Manual substrate control

```sh
effigy system up
effigy workspace
effigy dev
effigy system down
```

Behavior:

- user explicitly owns substrate lifecycle
- later `workspace` and `dev` leave the running system alone

## 12) Implementation Notes

This contract implies the following product work:

- add `system` command family
- add manifest-backed `systems` and `workspaces` naming contract
- support default-system and default-workspace resolution
- support workspace-level inline container sugar with canonical normalization
- route existing public container lifecycle surfaces onto `system`
- keep `dev` as task-owned concurrent runtime
- add system ownership tracking for `dev`, `shell`, and `workspace`
- detect in-workspace execution so nested `dev` does not recurse
- keep dependency mutation on the interactive workspace surface, not hidden in
  `dev`

This contract does not require:

- a new app-runtime built-in
- a different manifest model for repo-owned `dev` tasks
- hidden package-manager automation during normal startup

## Expected Outcome

After this contract is implemented, Effigy should feel normal on
container-backed repos:

- `dev` still means "run my app concurrent"
- `system` means "own the substrate lifecycle"
- `workspace` means "log me into the Linux-native dev environment"
- dependency mutation happens on an explicit maintenance surface, not as
  surprise startup side effects

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`../architecture/020-container-infrastructure-design.md`](../architecture/020-container-infrastructure-design.md)

## Next Step

Use this contract as the source of truth when deciding whether a new
container-backed repo should lead with `[systems.<name>]` + `effigy system`
(preferred for multi-service substrate) or `[containers.<name>]` +
`effigy container` (preferred for a single compose environment with no
surrounding system wiring). Cross-link from the repo's front-door guide
([`012-dev-process-manager-tui.md`](./012-dev-process-manager-tui.md)) when
tuning the developer-first experience.
