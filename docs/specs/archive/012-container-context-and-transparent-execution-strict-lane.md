# 012 Container Context and Transparent Execution Strict Lane

Status: complete
Updated: 2026-04-18
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

`complete`

The `effigy-exec` crate is shipped as an isolated library with 53 tests:

- **Routing engine** (`routing.rs`): determines host vs container execution
  for any command. Host-native allowlist for management commands (doctor,
  container, gateway, release, etc.). Per-task overrides: `run_in = "host"`,
  plus explicit workspace/container targeting through resolved task binding.
  Container-not-running
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
crates. The remaining work is product integration, but it now depends on the
`g02.011` service-catalog/container integration spine rather than on
`g02.010`.

## Integration Constraint

The isolated crate work is already shipped, and `g02.011` is now complete.
The next work here is product integration on top of that now-real container
surface:

- start with bounded routing integration before adding the wider exec surface
- keep routing, aliases, and handoff as separate batches instead of one large
  runner rewrite
- preserve the clean shell/domain boundary established in `g02.010`

## Remaining Integration Work

The bounded continuation chain is now fully landed:

1. `264` — context-routing foundation: manifest context support plus
   `effigy-exec::routing::route()` in normal task dispatch
2. `265` — explicit exec and alias surface: manifest `exec` support,
   `effigy exec`, CWD mapping, container handoff, and alias fallback

What is now real in the product path:

- `containers.*.context = "dev"` in the manifest
- `tasks.*.run_in = "host"` as a host-routing override
- standard task dispatch routes through `effigy-exec::routing::route()`
- standard routed tasks execute through the container path instead of assuming
  host-only shell execution
- stopped routed containers fail with a clear product error
- `effigy exec` routes ad-hoc commands through the dev container
- exec aliases resolve from the manifest, including bare-command fallback
- routed task execution preserves CWD semantics and chooses handoff vs raw exec
- `underlay-reference` proves explicit exec, alias fallback, and routed-task
  execution on a real consumer repo

## Exit Condition

This strict lane is complete when:

- `effigy-exec` is integrated into the runner
- `effigy test` transparently routes through the dev container
- `effigy exec` works for ad-hoc commands
- exec aliases resolve from the manifest
- one real project proves the loop

All exit conditions are now met.

## Next Task

This lane is closed. Return to planning and choose the next bounded integration
card from the remaining active roadmap lanes.
