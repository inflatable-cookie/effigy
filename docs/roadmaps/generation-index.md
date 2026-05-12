# Roadmap Generation Index

Current generation: `g05`
Updated: 2026-05-12

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
  - Held the production deployment export and runtime hardening generation.
  - Landed provider export foundations, runtime context, container manager,
    canonical task execution request, dependability proof, contract promotion,
    and artifact seed/apply/capture substrate work.
- `g04`
  - Held the runtime architecture simplification generation.
  - `001` lands the architecture sanity audit and opens the new queue.
  - The completed roadmap set focused on ownership purity for execution,
    runtime activation, container operations, data seed/dump, Rhai host APIs,
    drift guards, state stacks, deployment transactions, provider packages,
    post-release deduplication, and artifact/crate-boundary cleanup.
  - Closed through `g04.039` after the post-v0.6.x reference-grade cleanup
    sweep.

## Current Planning State

`g05` is active as the secret and local configuration management generation.
`g05.001`, `g05.002`, and `g05.003` are complete.

Current ready work:

- execute ready card `717`
- add container secret injection
- keep container startup injection blocked until `g05.005`

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
- purge stale generation-specific specs from `docs/specs/` so
  the active planning tree no longer carries dead lane debris

If that cleanup has not happened, stay in the current generation and finish the
closeout there first.

## Next Task

Execute `717` to add container secret injection.
