# Roadmaps

Roadmaps are executable milestone plans derived from Effigy vision and architecture.

## Generation model

- Use generation folders: `g01`, `g02`, `g03`, `g04`, `g05`, and future `gNN`
  generations opened by explicit rollover.
- Use milestone files inside each generation: `NNN-<slug>.md`.
- Reference milestones as `gNN.NNN`.
- Trigger generation rollover manually; do not use automatic file-count limits.
- Treat generations as substantial sequencing eras, not one-or-two-file
  buckets. As a healthy default, expect roughly 20 to 40 roadmap files in one
  generation before rollover is even worth discussing.
- Treat rollover as full generation closeout, not a convenience reset:
  close, supersede, or rehome every roadmap in the current generation first,
  then purge stale generation-specific specs from
  `docs/specs/` before opening the next generation.

## Layout

- `gNN/batch-cards/` optional per-generation execution cards
- `g05/` current secret and local configuration management generation
- `g04/` previous runtime architecture simplification generation
- `g03/` previous production export and runtime hardening generation
- `g02/` previous release and local-runtime expansion generation
- `g01/` original implementation and consolidation generation
- `generation-index.md` active generation and rollover history
- `backlog/` deferred scope with promotion criteria

## Current queue

- `g01` is closed as the original implementation and consolidation generation.
- `g02` is closed as the release and local-runtime expansion generation.
- `g03` is closed as the production export and runtime hardening generation.
- `g04` is complete through
  [`g04.039`](./g04/039-artifact-and-crate-boundary-rejustification.md). It
  covered runtime architecture simplification, state stacks, deployment
  transactions, provider packages, post-release reference-grade cleanup, and
  artifact/crate-boundary rejustification.
- `g05` remains the current generation. `g05.001` through `g05.007` remain
  complete from the secret and local configuration management suite,
  `g05.008` through `g05.015` completed the post-release ownership and
  maintainability cleanup follow-through, and `g05.016` through `g05.019` are
  now queued for schema-shape consolidation.

## Active Strict Lane

- [`082-manifest-section-schema-owner-convergence-strict-lane.md`](../specs/082-manifest-section-schema-owner-convergence-strict-lane.md)

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

## Rollover guardrail

Do not open `gNN+1` while the current generation still has live roadmap files
or stale strict-lane debris in the active specs tree.

Before rollover:

- every roadmap in the closing generation must be explicitly closed, paused,
  superseded, or moved to backlog
- the roadmap front doors must agree that the old generation is no longer the
  live queue
- `docs/specs/` must be purged so only live or near-live planning artifacts
  remain in the active tree

## Next Task

Execute `737` inside strict lane `082` and continue through the bounded ready
chain while each card closes green.

## Historical language boundary

- New roadmaps and actively maintained roadmap updates must use roadmap IDs and batch language.
- Older imported roadmap bodies may retain internal `Phase X.Y` execution headings as historical record.
- Leave those historical headings alone unless that roadmap is reopened for active work, then normalize it in the same batch.

## Historical command boundary

- older roadmap bodies may retain wrapper-script names or superseded command
  spellings when they describe the implementation path that existed at the time
- treat those references as historical planning evidence, not current operator
  guidance
- active release/runtime usage should be taken from the guides, contracts, and
  current roadmap front matter rather than old roadmap body details
