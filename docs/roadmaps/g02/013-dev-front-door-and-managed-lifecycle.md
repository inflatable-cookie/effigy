# 013 - Dev Front Door and Managed Lifecycle

Generation: `g02`

Status: Complete
Owner: Platform
Created: 2026-04-16
Depends on: 011, 012

## Vision Alignment

With catalog-based compose assembly and transparent container execution, the
remaining DX gap is startup friction. Starting a dev environment should be one
command that fires up the container, starts the gateway, waits for health, and
presents a TUI with an embedded terminal. Closing the TUI should shut
everything down.

## Primary Tags

- `OPERATE`
- `CONTRACT`

## Target Envelope

- `effigy dev` is a single-command front door for project development.
- It uses the managed-process concurrent runtime for a multi-tab TUI.
- Container lifecycle is the managed process — start on enter, stop on exit.
- Gateway auto-starts if DNS is configured.
- Health-check gate with "ready" indicator.
- Embedded terminal tab for container shell access.
- Closing the TUI triggers graceful shutdown.

## Vision Target Delta

- Move from `manual container lifecycle plus separate terminal windows` toward
  `one-command dev environment with integrated feedback and terminal`.

## 1) Problem

Even with transparent execution routing, the developer still needs to:

1. Start the container.
2. Optionally start the gateway.
3. Wait for services to be healthy.
4. Open a terminal for container work.
5. Remember to shut down when done.

That's five steps that should be one.

## 2) Goals

- [x] Define the `effigy dev` task pattern using managed-process concurrent
      runtime.
- [x] Define `tasks.dev.managed` section for gateway auto-start, health wait,
      and ready message.
- [x] Implement container lifecycle as a managed process with shutdown-on-exit.
- [x] Implement embedded terminal tab (shell into primary service).
- [x] Implement health-check gate with TUI-visible "ready" indicator.
- [x] Implement gateway auto-start when `dns.domain` is configured.
- [x] One real project proof where `effigy dev` is the only command needed.

## 3) Non-Goals

- [ ] No gateway implementation in this milestone (uses `g02.014` gateway if
      available, skips cleanly if not).
- [ ] No persistence or data management (deferred to `g02.015`).
- [ ] `effigy dev` is a task-level convention, not a built-in command. Projects
      define their own `[tasks.dev]`.

## 4) Contract Direction

### 4.1 Task Definition

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks.dev]
workspace = "app"
mode = "attached"
gateway = true
health_wait = true
ready_message = "http://projectname.test"

[[tasks.dev.concurrent]]
name = "services"
label = "Container"
role = "lifecycle"
shutdown_on_exit = true

[[tasks.dev.concurrent]]
name = "terminal"
label = "Shell"
role = "shell"
```

### 4.2 Managed Section

New fields under `tasks.<name>.managed`:

- `gateway` — boolean, auto-start gateway if DNS is configured for the
  container
- `health_wait` — boolean, wait for the container health check before showing
  ready
- `ready_message` — string, displayed when health passes (typically the URL)

### 4.3 Shell Role

The `role = "shell"` concurrent entry opens an interactive terminal session
inside the container's primary service. This uses the same exec path as
`effigy container shell` but embedded in the TUI.

### 4.4 Lifecycle Role

The `role = "lifecycle"` concurrent entry owns the container process.
`shutdown_on_exit = true` means closing this tab (or the TUI) triggers
container shutdown.

### 4.5 Relationship to v1

This builds directly on v1 attached sessions. The difference is:

- v1: `effigy container up` opens a log-follow TUI
- This: `effigy dev` opens a managed TUI with terminal, health gate, and
  gateway integration

The managed-process concurrent runtime from `g02.010` provides the tab model.

## 5) Implementation Approach

### 5.1 Crate Impact

Depends heavily on the managed-process runtime from `g02.010` modularization.
New logic sits in `effigy-containers` (lifecycle management) and the TUI layer
(shell embedding).

### 5.2 Testing Strategy

- Integration test that starts a container, verifies health gate, verifies
  TUI state.
- Real-project proof where `effigy dev` is the daily driver.

## 6) Outcome

`g02.013` is complete.

What shipped on the product path:

- manifest-owned managed dev-task metadata through `tasks.<name>.managed`
- lifecycle-role ownership for workspace-backed container startup and shutdown
- shell-role embedding through the shipped primary-service container shell path
- readiness gating plus ready-message projection on the managed runtime path
- gateway auto-start through the shipped `effigy gateway up` surface when the
  resolved workspace container declares DNS
- one real-project proof in `underlay-reference` that `effigy dev` can replace
  the prior multi-command startup routine on a trustworthy boundary

What the final proof exposed:

- the consumer repo carried stale local API DB wiring that no longer matched
  its compose-owned Postgres service
- that gap was fixed in-batch in the consumer repo and did not require another
  Effigy roadmap batch

## Next Task

No further execution lives on this roadmap item. Stop in planning and choose
the next remaining `g02` lane deliberately.
