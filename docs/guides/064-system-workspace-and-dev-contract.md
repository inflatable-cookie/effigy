# 064 - System, Workspace, and Dev

Use this guide when the question is not "which command do I run?" but "how is
Effigy supposed to think about host-clean local dev?"

This is the mental-model guide for:

- `effigy system ...`
- `effigy workspace`
- `dev` tasks that bind to a system
- the relationship between those commands and `effigy container ...`

Use:
- this guide for the model and naming rules
- [`063-container-system-guide.md`](./063-container-system-guide.md) for the
  direct container commands
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md) for copy-paste
  manifest patterns

## Start Here

Use this page when you want to answer any of these:

- does `dev` keep owning the app runtime?
- how does a repo bring infra up without launching the whole app stack?
- where do package installs, updates, migrations, and one-off Linux-native
  maintenance happen?
- what is the relationship between `container` and `system` as public
  commands?

Short answer:

- `dev` keeps its historic role as the concurrent runner
- `system` is the infra lifecycle command
- `workspace` is the Linux-native maintenance command
- `container` remains the direct compose and data-lifecycle command

## Prereqs and feature-scoped dependencies (macOS)

- **Containers / systems** require **Colima**:

```bash
brew install colima
```

- **Local HTTPS** (when a repo uses `tls = true` for gateway routes) requires
  **mkcert** plus the one-time trust install:

```bash
brew install mkcert
mkcert -install
```

Docker is optional; install it only when you want the `docker` CLI on the host:

```bash
brew install docker docker-compose
```

## The Model

Effigy local dev has three layers:

- `system`
  - owns infra lifecycle: VM, compose, gateway, workspace handoff
- `workspace`
  - gets you into the Linux-native maintenance environment inside that system
- `dev`
  - runs the app runtime against the resolved system

That split matters because infra lifecycle and app lifecycle are not the
same thing:

- infra: database, object storage, mail, gateway, workspace container
- app runtime: API, jobs, Vite servers, watchers, shells, tabs

Effigy keeps them separate so package installs, repairs, migrations, and other
maintenance work do not have to hide inside normal app startup.

## Command Roles

### `effigy system ...`

This is the public infra lifecycle command:

```sh
effigy system up
effigy system down
effigy system status
effigy system logs
```

Use it when you want to:

- bring the infra up first
- inspect or repair infra without launching the app
- keep the system running across several commands

### `effigy workspace`

This is the Linux-native maintenance entrypoint.

Contract:

1. ensure the selected system is up
2. open the resolved workspace shell
3. leave a pre-existing system alone on exit
4. apply closeout policy only if this invocation started the system

Use it for:

- package installs and updates
- migrations
- cache cleanup
- one-off maintenance commands
- direct investigation inside the runtime environment

### `effigy dev`

`dev` keeps its historic meaning:

- resolve the repo's `dev` task
- run the repo's `concurrent` shape
- do not turn `dev` into a hidden system-repair or package-management command

If `dev` requires a system and the system is down, Effigy may bring it up
first. On exit, it should only close that system back down if this `dev`
invocation started it.

## Ownership Rules

The key distinction is simple:

- system was already running before this command
- system was started by this command

That rule applies to both `dev` and `workspace`.

### `effigy system up`

- idempotent
- if already up, report that cleanly
- if down, bring it up and leave it running

### `effigy dev`

Contract:

1. resolve the normal repo-owned `dev` task
2. auto-start the required system if it is down
3. record whether this invocation started the system
4. on exit:
   - stop it only if this invocation started it and policy says to
   - otherwise leave it alone

Default closeout policy for auto-started systems:

- `auto-stop`

### `effigy workspace`

Contract:

1. ensure the system exists
2. open the workspace shell
3. record whether this invocation started the system
4. on exit:
   - stop it only if this invocation started it and policy says to
   - otherwise leave it alone

That makes `workspace` behave like a leased session into the runtime
environment.

## Everyday DX Rule

Dependency mutation belongs on the explicit maintenance command, not hidden
inside `dev`.

Allowed `dev` behavior:

- validate that required infra exists
- auto-start missing infra when needed
- perform small, deterministic startup prep that is clearly part of app launch

Bad default `dev` behavior:

- large implicit package installs
- hidden dependency graph mutation
- open-ended repair loops
- repo-specific setup heuristics that surprise the user during normal startup

Typical pattern:

```sh
effigy workspace
cd /workspace-root/<repo>/acme-front
bun add <pkg>
exit
effigy dev
```

## Manifest Model

The public manifest language should match the command language:

- `system`
  - composed infra environment
- `workspace`
  - named execution space inside a system
- `container`
  - one implementation building block a workspace may use

Public examples should lead with `systems` and `workspaces`, not with lower
level container wiring.

### Canonical Shape

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

[tasks.frontend:install]
workspace = "node"
run = "bun install"

[tasks.smoke]
system = "ci"
workspace = "app"
run = "./scripts/smoke.sh"
```

Resolution order:

1. task `system`
2. repo `systems.default`
3. task `workspace`
4. system `default_workspace`

If resolution fails at any step, Effigy should fail clearly instead of
guessing.

### Simple Repo Shortcut

Simple repos do not need to spell out every layer separately.

Shortcut form:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }
```

Rules:

- this is only sugar
- the loader should normalize it into the same canonical model
- runtime and validation should still reason about one underlying shape

## `container` vs `system`

Both command families are still public, but they do different jobs.

- `system`
  - owns infra lifecycle for `[systems.<name>]`
  - includes VM/profile, compose, gateway, workspace handoff, and recovery
- `container`
  - owns direct compose lifecycle and data lifecycle for
    `[containers.<name>]`
  - still makes sense for repos that do not need a surrounding system

Lead new container-backed repos toward:

- `system`
- `workspace`

Reach for `container` directly when:

- the repo only has a simple compose environment
- you need data export, import, reset, or eject surfaces
- you need cross-project compose views such as `status --global`

## Expected User Flows

### One-shot daily startup

```sh
effigy dev
```

Behavior:

- auto-start system if needed
- run the repo-owned dev concurrent
- auto-stop system on exit only if this invocation started it

### Linux-native maintenance session

```sh
effigy workspace
```

Behavior:

- ensure system exists
- open workspace shell
- do package or maintenance work inside Linux
- auto-stop only if this invocation started the system

### Manual substrate control

```sh
effigy system up
effigy workspace
effigy dev
effigy system down
```

Behavior:

- user explicitly owns infra lifecycle
- later `workspace` and `dev` leave the running system alone

## Maintainer Notes

This page should stay focused on the model, not implementation archaeology.

If a future change does not alter one of these public ideas:

- `dev` owns app runtime
- `system` owns infra lifecycle
- `workspace` owns Linux-native maintenance access
- `container` remains the lower-level compose and data path

then it probably belongs in architecture docs or command-specific guides, not
here.

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`../architecture/020-container-infrastructure-design.md`](../architecture/020-container-infrastructure-design.md)

## Next Step

Use this page when deciding whether a new local-dev command belongs on
`system`, `workspace`, `dev`, or `container` before adding another command or
manifest knob.
