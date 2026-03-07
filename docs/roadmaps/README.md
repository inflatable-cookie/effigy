# Roadmaps

Roadmaps are executable milestone plans derived from Effigy vision and architecture.

## Generation model

- Use generation folders: `g01`, `g02`, `g03`.
- Use milestone files inside each generation: `NNN-<slug>.md`.
- Reference milestones as `gNN.NNN`.
- Trigger generation rollover manually; do not use automatic file-count limits.

## Layout

- `g01/` current roadmap generation
- `generation-index.md` active generation and rollover history
- `backlog/` deferred scope with promotion criteria

## Current queue

- `g01/001` through `g01/012` capture the delivered Effigy implementation baseline.
- `g01/013-effigy-northstar-doctrine-alignment.md` records the docs structure migration and is complete.
- `g01/014-attention-marker-scan-and-doctor-integration.md` is complete.
- `g01/015-effigy-self-hosting-and-agent-first-adoption.md` is complete.
- `g01/016-duplicate-blocks-scan-and-doctor-integration.md` is complete.
- `g01/017-comment-ratio-scan-and-doctor-integration.md` is complete.
- `g01/018-generated-in-src-scan-and-doctor-integration.md` is complete.
- `g01/019-stale-suppressions-scan-and-doctor-integration.md` is complete.
- `g01/020-research-phase-1-core-execution.md` is the first research roadmap (planned).
- `g01/021-research-phase-2-developer-experience.md` is the second research roadmap (planned).
- `g01/022-research-phase-3-scale-and-integration.md` is the third research roadmap (planned).
- `g01/023-builtin-test-suite-lifecycle-and-env.md` is the next implementation roadmap (planned).

## Research Program

Three-phase comparative research program:
- **Phase 1 (020)**: Core Execution — Configuration, caching, watch mode, DAG, TUI
- **Phase 2 (021)**: Developer Experience — Completions, errors, workspaces, portability
- **Phase 3 (022)**: Scale & Integration — Remote execution, CI/CD, IDE, plugins, telemetry

See `docs/research/README.md` for the research operating model.

## Backlog

Deferred roadmap items live in [backlog/README.md](./backlog/README.md).

## Batch and logging rule

- Execute milestones in meaningful batches.
- Create logs per completed batch or update cycle, not per individual task.

## Next Task

Roadmap `g01.019` is complete. The next planned milestones are the three-phase research program:

1. **g01.020** - Research Phase 1: Core Execution — Begin with Make, Just, and Task dossiers
2. **g01.021** - Research Phase 2: Developer Experience — Shell completions, error patterns
3. **g01.022** - Research Phase 3: Scale & Integration — Remote execution, CI/CD
4. **g01.023** - Builtin Test Suite Lifecycle and Environment — Declarative setup/teardown/env for builtin test

Open `g01.023` for the next implementation milestone, starting with the builtin test suite schema contract and shared env-resolution reuse.


## Historical language boundary

- New roadmaps and actively maintained roadmap updates must use roadmap IDs and batch language.
- Older imported roadmap bodies may retain internal `Phase X.Y` execution headings as historical record.
- Leave those historical headings alone unless that roadmap is reopened for active work, then normalize it in the same batch.
