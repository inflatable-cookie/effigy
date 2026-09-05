# Roadmaps

Roadmaps are executable milestone plans derived from Effigy vision and architecture.

## Generation model

- Use generation folders: `g01` through `g09`, and future `gNN` generations
  opened by explicit rollover.
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
- `g09/` current command-surface compaction and migration generation
- `g08/` closed graph-aware scan, hardening, dependency, documentation, and
  catalog-pack generation
- `g07/` previous native code graph intelligence generation
- `g06/` previous codebase lean-down and ownership simplification generation
- `g05/` previous secret and reusable-core hardening generation
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
- `g05` is closed through
  [`g05.027`](./g05/027-process-execution-boundary-review.md). It covered
  secret and local configuration management, post-release ownership cleanup,
  schema-shape consolidation, and reusable-core hardening.
- `g06` is closed through
  [`g06.008`](./g06/008-runner-private-domain-logic-reduction.md). It covered
  the post-`v0.7.0` codebase lean-down suite: state and release god-file
  reduction, test fixture convergence, CLI/help deduplication, typed
  contract-shape reuse, compatibility-branch deletion, and runner-private
  domain-logic reduction.
- `g07` is closed through
  [`g07.078`](./g07/078-graph-agent-adoption-closeout.md). It has covered the
  native code graph intelligence suite, follow-up graph performance/parity
  lanes, init setup-wizard work, bounded codebase leanness cleanup, and the
  residual-maintainability follow-through:
  warning-only god-file reduction, stubborn duplicate-block follow-through,
  graph test-harness decomposition, script-command boundary cleanup, bounded
  runner-private helper convergence, closeout proof, and now the graph
  agent-adoption follow-through focused on cross-repo freshness trust,
  behavioral query ranking, edit-target packets, benchmark proof, and agent
  guidance.
- `g08` is closed through
  [`g08.048`](./g08/048-catalog-pack-publication-and-cutover.md). It covered
  graph-aware scan intelligence, the code-quality boundary sweep and dead-code
  burn-down, the 2026-06-10 security and posture hardening suite, local
  dependency management (`effigy deps`), release-tag integrity and patch-lane
  hardening, papercuts discovery, explicit catalog membership (contract `037`),
  unified test orchestration (contract `038`), pre-release CI proof (contract
  `039`), committed Bun pinning (contract `040`), vision governance
  operationalization, documentation coverage and parity refreshes, the
  repository-defined documentation graph (contract `041`), the external skill
  task runner, help-first command discovery, a run of bounded papercut repairs,
  and catalog-pack acquisition, publication, and cutover. Per-milestone detail
  lives in [`g08/README.md`](./g08/README.md) and the archived logs.
- `g09` is the active generation.
  Completed [`g09.001`](./g09/001-command-surface-compaction-preview.md)
  shipped the operator-approved additive command-surface preview (card
  `1109`); strict spec `116` is archived. Live use rejected executable help
  namespaces, so [`g09.002`](./g09/002-flat-command-execution.md) restored
  direct canonical execution (card `1110`); strict spec `117` is archived.
  Completed [`g09.003`](./g09/003-acowtancy-consumer-adoption-replay.md) carried
  Theme 3 through strict spec `118`; card `1111` executed the frozen,
  read-only Acowtancy replay and published the first populated comparison
  scorecard. PR `88` merged at `9c05a883`.
  Completed [`g09.004`](./g09/004-release-gate-diagnosability.md) makes a
  failed release gate diagnosable from persisted output and environment
  records; card `1112` merged in PR `90` and spec `119` is archived.
  Completed [`g09.005`](./g09/005-docs-context-latency-and-freshness.md)
  repaired `docs context` warm and stale latency to frozen budgets; card
  `1113` merged in PR `91` and spec `120` is archived.
  Ready [`g09.007`](./g09/007-docs-context-exact-identifier-retrieval.md)
  makes exact identifier queries find their containing section under strict
  spec `121`; card `1114` is ready. Queued
  [`g09.006`](./g09/006-cross-repository-source-routing.md) routes one
  query across opted-in repositories under named directories, grouped per
  repository with identity, under strict spec `122` (card `1115`, serial
  after `1114`).

## Active Strict Lane

Strict spec [`121`](../specs/121-docs-context-exact-identifier-retrieval-strict-lane.md)
is active for [`g09.007`](./g09/007-docs-context-exact-identifier-retrieval.md).
Card [`1114`](./g09/batch-cards/1114-docs-context-exact-identifier-retrieval.md)
is the ready card; its dispatch manifest is in the roadmap. Strict spec
[`122`](../specs/122-cross-repository-source-routing-strict-lane.md) and card
[`1115`](./g09/batch-cards/1115-cross-repository-source-routing.md) are queued
serially behind `1114`. Specs `119` and `120` are archived. Direct invocation remains canonical and help grouping
remains.

## Research Program

Three-phase comparative research program:
- **Phase 1:** Core Execution — Configuration, caching, watch mode, DAG, TUI
- **Phase 2:** Developer Experience — Completions, errors, workspaces, portability
- **Phase 3:** Scale & Integration — Remote execution, CI/CD, IDE, plugins, telemetry

See `docs/research/README.md` for the research operating model.

## Backlog

Deferred roadmap items live in [backlog/README.md](./backlog/README.md).

## Batch and logging rule

- Execute milestones in meaningful batches.
- Create logs per completed batch or update cycle, not per individual task.

## Retention and archival convention

Keep the session-loaded surfaces lean; let closed history rest in archives.

- **Logs.** The active log index ([`../logs/README.md`](../logs/README.md))
  carries only the current generation's month window. When a generation closes,
  move its month directories under `docs/logs/archive/<month>/` and trim the
  index. Archived logs stay in the repo (and git history) as durable evidence;
  the default `effigy docs check index` excludes `archive/**`. Never delete a
  log to compact — move it.
- **Roadmaps.** Closed-generation milestone files and their nested
  `batch-cards/` stay in place: they are the planning record, are not loaded
  into the front doors, and moving them would churn hundreds of files and links
  for no signal gain. Compaction targets the indexes and front doors, not the
  per-generation history.
- **Front doors.** This README, `generation-index.md`, and the generation
  READMEs are the navigable surface. Keep them pointed at the live generation
  with closed generations summarized, not enumerated card-by-card.

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

Execute card `1114` (docs context exact identifier retrieval), then card
`1115` (cross-repository source routing). The consumer cohort
checkpoint is deferred by operator direction on 2026-09-05. Keep Acowtancy
read-only and Effigy release execution and S3 extraction behind their named
gates.

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
