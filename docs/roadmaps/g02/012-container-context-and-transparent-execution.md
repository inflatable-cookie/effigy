# 012 - Container Context and Transparent Execution

Generation: `g02`

Status: Complete (transparent container execution is now product-real and consumer-proven)
Owner: Platform
Created: 2026-04-16
Depends on: 006, 011

## Vision Alignment

The v1 container surface requires explicit `effigy container shell --command`
invocations to run anything inside a container. For projects where most work
happens inside the container, this friction makes the container feel like a
separate system rather than a transparent execution layer.

The next product problem is making the container invisible to normal workflow.
A project should be able to mark a container as its execution context so that
`effigy test` runs inside the container automatically.

## Primary Tags

- `ROUTE`
- `OPERATE`
- `CONTRACT`

## Target Envelope

- A container can be marked as the project's execution context.
- Task execution implicitly routes through the context container when running.
- Host-native commands (doctor, container management, release) stay on the
  host.
- `effigy exec` provides explicit ad-hoc command routing.
- Exec aliases provide shorthand for interactive tool access.
- CWD mapping translates host paths to container paths.
- If effigy is installed in the container, host effigy can hand off entirely.

## Vision Target Delta

- Move from `explicit container shell invocations for every command` toward
  `transparent execution routing where the container is the implicit runtime`.

## 1) Problem

In v1, the container is a visible layer. Running a test suite means:

```bash
effigy container web shell --command "php artisan test"
```

This is correct but high-friction. In a project where PHP, composer, and all
tools are in the container, almost every command needs this wrapper.

## 2) Goals

- [x] Define `context = "dev"` manifest field for marking the execution context.
- [x] Define routing rules: which commands route through the container, which
      stay on the host.
- [x] Implement CWD mapping (host path -> container working directory).
- [x] Implement `effigy exec <command>` for explicit container execution.
- [x] Implement exec alias declarations in the manifest.
- [x] Define effigy-in-container handoff protocol.
- [x] Define behavior when the context container is not running (prompt vs
      auto-start vs error).

## 3) Non-Goals

- [ ] No `effigy dev` TUI in this milestone (deferred to `g02.013`).
- [ ] No gateway or DNS integration (deferred to `g02.014`).
- [ ] No auto-start policy in the first proof — default to error with a clear
      suggestion to run `effigy container up`.

## 4) Contract Direction

### 4.1 Context Declaration

```toml
[containers.web]
context = "dev"
```

Only one container per project may be `context = "dev"`. Validation fails if
multiple containers claim the same context.

### 4.2 Routing Rules

Commands that always run on the host:

- `effigy doctor`
- `effigy container *`
- `effigy gateway *`
- `effigy service *`
- `effigy release *`
- `effigy tasks`
- `effigy help`
- `effigy version`

Commands that route through the context container:

- `effigy <task>` (any task invocation)
- `effigy exec <command>`

Individual task overrides:

- `host = true` — forces host execution
- `workspace = "<name>"` — targets a different resolved workspace/container
- `system = "<name>"` — switches the task onto another declared system before
  workspace resolution

### 4.3 CWD Mapping

When the user is at `~/projects/client/app/Models/` on the host, and the repo
root maps to `/var/www/html` in the container:

- effigy resolves the relative path from repo root: `app/Models/`
- exec runs with CWD `/var/www/html/app/Models/`

Mapping uses the container's `exec.working_dir` or the mount configuration.

### 4.4 Effigy-in-Container Handoff

If the container image has effigy installed:

1. Host effigy detects effigy binary inside the container.
2. Host effigy invokes `docker compose exec <service> effigy <args>`.
3. Container effigy runs natively — no CWD mapping needed, paths resolve
   naturally, full access to runtime environment.

If effigy is not in the container:

1. Host effigy resolves the raw command from the task definition.
2. Host effigy invokes `docker compose exec -w <cwd> <service> <command>`.
3. Stdin/stdout/stderr pass through.

Detection method: `docker compose exec <service> which effigy` or similar
probe, cached per container session.

### 4.5 Exec Aliases

```toml
[containers.web.exec.aliases]
mysql = { service = "db", command = "mysql" }
redis-cli = { service = "cache", command = "redis-cli" }
```

`effigy mysql` resolves to `docker compose exec db mysql`. These are for
interactive tool access, not for routing project tasks.

## 5) Implementation Approach

### 5.1 Crate Impact

- `effigy-containers` extension: context routing logic, exec proxy, CWD
  mapping.
- `effigy-manifest` extension: context field, exec config, alias
  declarations.
- `effigy-cli` extension: `exec` command surface.

These extensions happen after `g02.010` modularization completes. Library logic
(CWD mapping, alias resolution) can be developed in advance within
`effigy-catalog` or a new lightweight crate.

### 5.2 Testing Strategy

- Unit tests for CWD mapping logic.
- Unit tests for routing rule resolution.
- Integration tests with a running container verifying exec passthrough.

## Landed State

- `effigy-exec` ships the routing, CWD mapping, alias, and handoff logic as a
  clean library boundary.
- Manifest `context = "dev"` and `[containers.<name>.exec]` are wired through
  CLI, schema, and runner integration.
- Standard task execution routes through the dev container when appropriate and
  preserves working-directory semantics across host and container.
- `effigy exec` is a first-class product command.
- Bare alias commands resolve through the same container exec surface.
- `underlay-reference` proves explicit exec, alias fallback, and routed-task
  execution on a real consumer repo.

## Next Task

`g02.012` is complete. Return to planning and choose the next bounded
integration batch from the remaining active roadmap lanes.
