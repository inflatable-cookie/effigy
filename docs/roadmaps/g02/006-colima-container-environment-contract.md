# 006 - Colima Container Environment Contract

Generation: `g02`

Status: Complete
Owner: Platform
Created: 2026-04-15
Depends on: 002, 027

## Vision Alignment

Effigy can already coordinate tasks, demos, release work, and manifest-driven
repo policy, but web-oriented repos still depend on host-installed databases,
PHP runtimes, reverse proxies, and other local services that do not belong on
the maintainer's machine.

The next product problem is not “more dev tasks.” It is an explicit
container-environment surface that lets a repo describe one or more named
development environments, then lets Effigy bring them up, attach to them, and
tear them down as part of normal task sessions.

This lane exists because the user's current machine cannot work comfortably on
web projects until those services are encapsulated behind a Colima-based dev
environment contract.

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `ROUTE`
- `MAINT`

## Target Envelope

- Effigy ships a first-class `container` command surface instead of overloading
  `dev`.
- Repos can declare a registry of named container environments in the manifest.
- A repo can declare a default container so `effigy container up` remains
  ergonomic without making `effigy dev` special globally.
- Tasks can reference named container controls explicitly where needed.
- An attached container session can be owned by one Effigy task process so
  Ctrl+C or session close performs graceful container shutdown by default.
- The v1 driver is Colima, but the contract stays framed around named
  container environments rather than raw Colima VM commands.
- One real web-oriented consumer repo proves the loop end to end.

## Vision Target Delta

- Move from `repo-specific shell glue plus host-installed web services` toward
  `manifest-driven, attached container environments with explicit lifecycle
  ownership inside Effigy`.

## 1) Problem

Web repos need databases, app runtimes, reverse proxies, queues, and other
services. Today those tend to leak onto the host machine or get rebuilt as
ad hoc repo scripts with no common Effigy contract.

That creates four failures:

- local machines accumulate framework-specific service installs
- repo tasks cannot assume a consistent environment lifecycle
- attached operator feedback is weak or fragmented across terminals
- graceful teardown depends on human memory instead of a task/session owner

## 2) Goals

- [x] Define `effigy container` as a first-class command surface.
- [x] Define a manifest registry of named container environments.
- [x] Support a manifest-defined default container alias.
- [x] Let tasks reference named container controls explicitly.
      Shipped through the later `system` / `workspace` task contract.
- [x] Define an attached session model where one owner process governs
      container lifecycle and teardown.
- [ ] Reuse Effigy's multi-tab session model for status, logs, and service
      feedback where appropriate.
      Shipped for attached terminal sessions.
- [x] Keep the v1 driver focused on Colima-based web development.
- [x] Prove the contract in one real consumer repo before broad rollout.

## 3) Non-Goals

- [ ] No attempt to make `effigy dev` a universal built-in.
- [ ] No Kubernetes or multi-cluster orchestration in v1.
- [ ] No multi-driver parity in the first batch.
- [ ] No background daemon requirement if process-owned sessions are enough.
- [ ] No broad container abstraction for deployment or CI in this lane.

## 4) Contract Direction

### 4.1 Command Surface

Preferred command grammar:

```text
effigy container <name> up
effigy container <name> down
effigy container <name> status
effigy container <name> logs
effigy container <name> shell
effigy container <name> reset
```

Preferred default alias:

```text
effigy container up
effigy container down
effigy container status
```

Resolution rule:

- if `<name>` is omitted, Effigy resolves the manifest-defined default
  container
- if no default exists, Effigy fails clearly instead of guessing

### 4.2 Manifest Registry

Preferred root shape:

```toml
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "effigy"
compose_file = "infra/dev/docker-compose.yml"
project_name = "my-app-dev"
primary_service = "app"

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 10

[containers.web.health]
check = "http://localhost:8080/health"
timeout_secs = 60

[containers.web.host]
ports = ["8080:80", "3306:3306"]
mounts = ["./:/workspace"]

[containers.web.ui]
tabs = ["overview", "app", "db", "proxy"]
```

Rules:

- the registry is reusable outside one special `dev` task
- the default alias is optional
- task integration references named containers instead of embedding
  Colima/docker commands directly
- host-facing ports and mounts are explicit manifest policy, not implicit
  behavior inferred from compose files at runtime

### 4.3 Task Integration

Task integration should be explicit and repo-owned.

Preferred direction:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks]
dev = { workspace = "app" }
```

or, if lower-level composition is needed:

```toml
[tasks]
dev = [
  { container = "default", action = "up", mode = "attached" },
  { task = "app:dev" },
]
```

Decision rule for v1:

- keep the product surface centered on `effigy container ...`
- allow tasks to reference that surface
- do not make `dev` globally magical

### 4.4 Attached Session Ownership

This is the core lifecycle rule for v1:

- one Effigy task/session process can own one attached container environment
- when the owning process exits, Effigy applies the configured shutdown policy
- Ctrl+C, UI close, or explicit stop should all route through the same
  graceful teardown path
- default behavior should be shutdown-on-owner-exit, not leave-running

This gives repos a clean operator experience:

- `effigy dev` can still be the repo-level entrypoint if the repo wants it
- containers do not linger by accident
- lifecycle stays inspectable and bounded inside Effigy

### 4.5 TUI Direction

The attached session should reuse the multi-tab structure where helpful.

Minimum useful tabs:

- overview
- services
- logs
- shell
- events

The operator should be able to understand:

- whether Colima is up
- which services are healthy
- which ports are exposed
- what the owner task is
- what shutdown will happen on exit

## 5) Questions To Settle

### 5.1 Session shape

Need to decide whether v1 ships:

- only a dedicated `container up --attach` flow
- or a first-class task-owned attached-session primitive

Decision for `106`:

- v1 keeps `effigy container ...` as the primary product surface
- v1 does not require a first-class task-owned attached-session primitive before
  implementation starts
- repos may initially compose container control through ordinary task entries
  that reference named containers
- if the first implementation proof shows that attached-session ergonomics are
  too awkward in plain task composition, a dedicated task-level session
  primitive can become the next bounded follow-up rather than a hidden v1
  dependency

### 5.2 Service exposure back into the host

Decision for `106`:

V1 host/service integration is explicit and narrow:

- host access happens through manifest-declared `ports`, not implicit service
  discovery
- the host-facing access story is `localhost:<declared-port>`
- v1 does not invent host DNS names, host aliases, or global service
  registration
- file sharing happens through manifest-declared repo-relative `mounts`
- v1 should reject mounts that escape the repo root unless a later lane opens
  that boundary deliberately
- the primary interactive shell target is the manifest-declared
  `primary_service`
- additional service exec/log targets may exist by explicit service name, but
  Effigy should not guess one
- readiness may be declared via one manifest health check for the environment;
  v1 does not need a full per-service health DSL before the first proof

That settles the execution assumptions:

- host port mapping policy
- host path mounts and write semantics
- no host DNS invention
- explicit primary shell target
- one narrow environment health gate

### 5.3 Colima scope

Decision for `106`:

- v1 allows a named `profile` field per container environment
- Effigy only has to support one active profile per attached session in the
  first proof
- v1 does not need broad concurrent multi-profile orchestration semantics
  beyond “start the referenced profile if needed and use it for this
  environment”

## 6) Current Foundation Status

`107` and `108` now ship the first trustworthy v1 implementation surface:

- first-class `effigy container` command family:
  - `up`
  - `down`
  - `status`
  - `logs`
  - `shell`
  - `reset`
- manifest-backed `[containers]` registry with:
  - optional default container
  - explicit `profile`
  - explicit `primary_service`
  - explicit host `ports`
  - repo-relative host `mounts`
  - one environment health gate
  - lifecycle policy for owner exit
- attached owner-session shutdown proof through targeted CLI tests
- attached terminal sessions now widen into an Effigy-owned multi-tab runtime
  with:
  - an `overview` tab
  - primary-service log follow
  - extra service tabs derived from `ui.tabs`
- repos can now expose one attached container session through
  task-owned workspace bindings without embedding raw compose commands
- no-Docker-host fallback through `colima nerdctl` plus Colima startup with
  `--runtime containerd`
- one honest consumer proof in `example-site`

Consumer-proof result:

- `example-site` adopted a `services` container registry entry over its
  existing `docker-compose.yml`
- detached bring-up succeeded on the real machine
- running status succeeded against live services
- graceful teardown succeeded
- repo-owned task composition now also launches honestly through
  `dev:services`
- the real proof stayed honest about one remaining edge:
  non-interactive live-stop behavior under a real `colima nerdctl compose`
  session is still less trustworthy than the targeted runtime tests
- the proof remained bounded to one safe consumer repo

Still open after the widened proof:

- the live-stop/operator-closeout path should either be accepted as
  test-backed-for-now or widened once more around the real `nerdctl` proof
  edge
- `down` intentionally tears down compose services but does not stop the Colima
  profile

## 7) Settled V1 Contract

The `106` decision settles the v1 contract strongly enough to implement:

- primary command surface: `effigy container ...`
- optional default container alias
- manifest registry of named environments under `[containers]`
- Colima-first driver with optional named profile reference
- explicit host-facing ports and repo-relative mounts
- explicit `primary_service` for shell/exec targeting
- one attached owner-session lifecycle with shutdown-on-owner-exit by default
- repo-owned task integration without making `effigy dev` globally special

That hardening batch is now shipped too:

- attached startup now honors stop requests before the live log-follow session
  begins
- inherited log-follow subprocess trees are now terminated as process groups
  instead of only killing the top-level wrapper
- the real `example-site` proof now exits cleanly under a timed `SIGINT`,
  applies graceful shutdown, and leaves the compose environment down while the
  Colima profile remains inspectable

## 8) Consumer-Proof Strategy

The first proof should be one real web-oriented repo that currently needs local
services on the host.

Proof expectations:

- no host-installed DB/server dependency for normal local work
- one honest `effigy container` proof path that exercises bring-up and teardown
- repo-level task composition can be added later if the first foundation proves
  the command surface first
- graceful teardown on Ctrl+C or UI close remains a required direction
- attached-session UX/TUI widening becomes the next bounded batch if the
  first proof is still too log-follow-shaped

## 9) Milestone Relationship

This lane is the highest-priority blocker once the current Effigy release work
around the optional distribution surface is closed.

It should therefore lead the next active sequence, while the broader
cross-project rollout milestones stay queued behind it.

## Next Task

This roadmap is complete on the current v1 container boundary.

Successor work now belongs in:

- `g02.011` service catalog and compose assembly integration
- `g02.012` transparent execution integration
- `g02.013` managed `effigy dev`
- `g02.014` gateway integration
- `g02.015` persistent data lifecycle
- `g02.016` multi-project coordination
