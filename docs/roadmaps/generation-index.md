# Roadmap Generation Index

Current generation: `g02`
Updated: 2026-04-16

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
  - `003` captures the demo harness model and runner/browser contract; it is complete and released in `v0.2.13`.
  - `004` captures the Rust-native scripting surface; it is paused after the shipped Rhai foundation, Effigy dogfooding, and native distribution cutover reached a clean internal boundary.
  - `005` captures the optional distribution surface; it is now paused after one real consumer proof plus bounded widening made the metadata-validation, artifact-validation, and closeout boundary trustworthy, while the fuller published-consumer `first-publish` question stays explicitly deferred.
  - `006` captures the Colima container environment surface; it is now paused after the shipped foundation, attached-session widening, repo-owned task composition, and real-machine live-stop hardening reached a trustworthy v1 boundary.
  - `007` captures the distribution release and consumer rollout closure work for the shipped optional distribution surface; local Linux rehearsal plus Rhai runtime hardening are now real, but the actual release-closure card is queued again while `010` continues shrinking the remaining TUI shell before `v0.3`.
  - `008` captures the remaining demo and manifest-import rollout across the intended repo cohort.
  - `009` captures the vault-backed rollout of the already-shipped env-schema / varlock foundation.
  - `010` captures Effigy's modularization and crate-boundary architecture lane, remains active, and has already shipped the backbone plus the main product-domain crates; the CLI shell, TUI/browser seams, demo seam, and changelog workspace seam are now extracted, and the current question is whether the remaining release shell is finally honest enough for release closure to resume.

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
