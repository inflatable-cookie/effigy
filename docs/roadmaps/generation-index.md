# Roadmap Generation Index

Current generation: `g03`
Updated: 2026-04-30

## Generation history

- `g01`
  - Holds the imported Effigy implementation roadmap corpus plus the first
    Northstar-aligned consolidation and tooling lanes.
- `g02`
  - Held the release and local-runtime expansion generation.
  - Landed the bootstrap, manifest-composition, demo, scripting, container,
    gateway, data, coordination, starter, release, and hardening work that led
    to `v0.3.0` and `v0.3.1`.
- `g03`
  - Starts the production deployment export generation.
  - `001` defines the neutral deployment model and export contract.
  - `002` proves Underlay managed-platform export.
  - `003` scopes the future Decodelabs production strategy without forcing it
    into the first delivery lane.

## Research Roadmaps

Three-phase research program covering comparative tool analysis:

- **Phase 1 (020)**: Core Execution — Configuration, caching, watch mode, DAG, TUI
- **Phase 2 (021)**: Developer Experience — Completions, errors, workspaces, portability, env
- **Phase 3 (022)**: Scale & Integration — Remote execution, CI/CD, IDE, plugins, telemetry

## Rollover rule

Start a new generation only when manually triggered because roadmap scope,
vision, or architecture has shifted enough to justify a fresh sequence.

Generations should be substantial. As a healthy default, expect something
closer to 20 to 40 roadmap files before rollover is worth discussing. Treat
that as a judgment guardrail, not an automatic counter.

Rollover is a closeout event, not a convenience move. Before opening the next
generation:

- close, pause, supersede, or rehome every roadmap in the current generation
- refresh the roadmap front doors so the old generation is visibly closed
- purge stale generation-specific specs and batch cards from `docs/specs/` so
  the active planning tree no longer carries dead lane debris

If that cleanup has not happened, stay in the current generation and finish the
closeout there first.
