# Roadmap Generation Index

Current generation: `g02`
Updated: 2026-04-11

## Generation history

- `g01`
  - Holds the imported Effigy implementation roadmap corpus `001` through `012`.
  - `013` captures the Northstar doctrine alignment batch.
  - `014` captures the completed attention-marker scan and doctor integration milestone.
  - `015` captures the completed self-hosting and agent-first adoption milestone.
  - `016` captures the completed duplicate-blocks scan and doctor integration milestone.
  - `017` captures the completed comment-ratio scan and doctor integration milestone.
  - `018` captures the completed generated-in-src scan and doctor integration milestone.
  - `019` captures the completed stale-suppressions scan and doctor integration milestone.
- `020` captures the planned Research Phase 1: Core Execution.
- `021` captures the planned Research Phase 2: Developer Experience.
- `022` captures the planned Research Phase 3: Scale and Integration.
- `023` captures the planned builtin test suite lifecycle and environment milestone.
- `024` captures the completed release pipeline validation and consumer CI integration milestone.
- `025` captures the env-schema integration milestone.
- `026` captures the changelog library and Northstar profile milestone.
- `027` captures the release orchestration milestone.
- `028` captures the completed script-surface reduction and built-ins milestone.
- `029` captures the active Northstar + Effigy consumer adoption and product-boundary milestone.
- `g02`
  - Starts a new product cycle after the implementation/consolidation-heavy `g01` sequence.
  - `001` captures the stateless bootstrap command and clone contract milestone, now complete: released and live-pilot validated on `loophole` and `songsprout`.
  - `002` captures the manifest composition and override contract; the foundation and inspectability surface are now shipped strongly enough for downstream planning to depend on.
  - `003` captures the active demo harness model and runner/browser contract so proof verification becomes a first-class Effigy surface; registry/inspection, the first `demo run` slice, and lifecycle targeting are now shipped.

## Research Roadmaps

Three-phase research program covering comparative tool analysis:

- **Phase 1 (020)**: Core Execution — Configuration, caching, watch mode, DAG, TUI
- **Phase 2 (021)**: Developer Experience — Completions, errors, workspaces, portability, env
- **Phase 3 (022)**: Scale & Integration — Remote execution, CI/CD, IDE, plugins, telemetry

## Rollover rule

Start a new generation only when manually triggered because roadmap scope,
vision, or architecture has shifted enough to justify a fresh sequence.
