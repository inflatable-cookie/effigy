# 012 Container Context and Transparent Execution Strict Lane

Status: active
Updated: 2026-04-16
Roadmap: `g02.012`

## Context

The v1 container surface requires explicit `effigy container shell --command`
invocations to run anything inside a container. For projects where most work
happens inside the container, this friction makes the container feel like a
separate system rather than a transparent execution layer.

This lane makes the container invisible to normal workflow by introducing
execution context routing.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/012-container-context-and-transparent-execution.md`
- `docs/architecture/020-container-infrastructure-design.md`

## Lane Focus

- define `context = "dev"` manifest field
- implement transparent task routing through the dev container
- implement CWD mapping for host-to-container path translation
- implement `effigy exec` for explicit ad-hoc command routing
- implement exec aliases for interactive tool access
- implement effigy-in-container detection and handoff
- define behavior when the container is not running

## Current Posture

`active`

The `effigy-exec` crate is shipped as an isolated library with 53 tests:

- **Routing engine** (`routing.rs`): determines host vs container execution
  for any command. Host-native allowlist for management commands (doctor,
  container, gateway, release, etc.). Per-task overrides: `host = true`,
  `container_session = "none"` or `"<name>"`. Container-not-running
  detection with clear user guidance. Decision struct with human-readable
  reason for every routing choice.

- **CWD mapping** (`cwd.rs`): bidirectional host-to-container and
  container-to-host path translation. Validates paths are inside repo root.
  Handles canonicalization with graceful fallback.

- **Exec aliases** (`alias.rs`): named shortcuts for service-specific
  commands. Multi-word command base support ("php artisan" + user args).
  Resolution with available-aliases error reporting.

- **Container detection** (`detection.rs`): probes whether effigy is
  installed inside the container. ExecStrategy enum: Handoff (effigy-to-
  effigy) vs RawExec (with CWD mapping). Capability cache with configurable
  expiry. Standard probe spec and result builder.

All logic is pure — no I/O, no Docker calls, no dependency on other effigy
crates. Integration into the manifest schema, CLI, and container command
dispatch happens after `g02.010` completes.

## Isolation Constraint

This lane writes only to `crates/effigy-exec/` (already shipped). Integration
into `src/`, `crates/effigy-containers/`, `crates/effigy-manifest/`, or
`crates/effigy-cli/` happens after `g02.010` modularization completes.

## Remaining Integration Work

When `g02.010` finishes, the integration path is:

1. Add `context` field to `ContainerConfig` in `effigy-manifest`.
2. Add `exec` and `exec.aliases` sections to `ContainerConfig`.
3. Add `host` field to `ManifestTask`.
4. Wire `effigy-exec::routing::route()` into the task dispatch path in the
   runner. Before executing a task, call `route()` to determine the target.
5. Wire `effigy-exec::cwd::CwdMapper` into the container exec path.
6. Wire `effigy-exec::detection` into container session startup to probe
   capabilities and cache them.
7. Add `effigy exec <command>` CLI subcommand that bypasses task routing
   and directly executes in the dev container.
8. Register exec aliases as pseudo-tasks in the task catalog so
   `effigy mysql` resolves through the alias table.

## Exit Condition

This strict lane is complete when:

- `effigy-exec` is integrated into the runner
- `effigy test` transparently routes through the dev container
- `effigy exec` works for ad-hoc commands
- exec aliases resolve from the manifest
- one real project proves the loop

## Next Task

Integration waits for `g02.010` to complete. The library crate is ready.
