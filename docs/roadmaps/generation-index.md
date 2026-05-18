# Roadmap Generation Index

Current generation: g07
Updated: 2026-05-18

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

`g05` is closed.

- `g05.001` through `g05.007` completed the secret and local configuration
  management generation.
- `g05.008` through `g05.015` completed the post-release ownership and
  maintainability cleanup follow-through identified by the earlier codebase
  sweep.
- `g05.016` through `g05.019` completed schema-shape consolidation across
  `[manifest]` and task-like definition owners.
- `g05.020` through `g05.027` completed the reusable-core hardening tranche
  from the 2026-05-14 sweep.

`g06` is closed.

- `g06.001` opens the codebase lean-down suite.
- `g06.002` through `g06.008` cover the first deletion-oriented lanes:
  state shell trim, release module reduction, fixture convergence, CLI/help
  deduplication, typed contract-shape reuse, compatibility-branch deletion,
  and runner-private domain-logic reduction.

`g07` is now the current active generation.

- `g07.001` opens the native code graph intelligence suite.
- `g07.002` through `g07.012` cover graph storage/contracts,
  indexing/freshness, first-party language extraction, Effigy manifest/docs
  indexing, query commands, agent context packs, and performance proof.
- `g07.013` through `g07.016` reopen graph work for incremental indexing,
  query-speed reduction, and failed fixture-path reliability.
- `g07.017` through `g07.020` completed the bounded file-walk and scan-cost
  reduction pass after the larger extractor/query wins landed.
- `g07.021` through `g07.024` completed the foreground watch-mode lane for
  bounded filesystem-event refresh and explicit reconcile fallback.
- `g07.025` through `g07.029` completed the graph context ranking-quality lane
  after the first practical usefulness assessment showed generic queries
  over-ranked tests/docs and direct `rg` remained better for exact text.
- The suite explicitly excludes MCP, a graph daemon, external language plugins,
  JavaScript runtime dependencies, and LLM-generated summaries as canonical
  graph data.

Current ready work: none.

## Research Roadmaps

Three-phase research program covering comparative tool analysis. These are
research phases, not `g05` roadmap IDs:

- **Phase 1:** Core Execution — Configuration, caching, watch mode, DAG, TUI
- **Phase 2:** Developer Experience — Completions, errors, workspaces, portability, env
- **Phase 3:** Scale & Integration — Remote execution, CI/CD, IDE, plugins, telemetry

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

No active `g07` execution card remains.
