# Roadmaps

Roadmaps are executable milestone plans derived from Effigy vision and architecture.

## Generation model

- Use generation folders: `g01` through `g08`, and future `gNN` generations
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
- `g08/` current graph-aware scan intelligence and code quality boundary
  follow-through generation
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
- `g08` is active. Milestones `g08.001` through
  [`g08.009`](./g08/009-code-quality-boundary-sweep-suite.md) are complete: the
  first tranche covered graph-aware scan intelligence (preserving deterministic
  filesystem scans, adding optional graph enrichment, and graph-native scans for
  boundaries, isolated code, hotspots, and validation gaps); the follow-up
  tranche covered the code-quality boundary sweep (descriptor convergence,
  container bring-up cleanup, repo-marker convergence, duplicate-block
  follow-through, boundary/dead-code self-adoption, and the dead-code burn-down
  to 0 findings). The current
  [`g08.010`](./g08/010-security-and-posture-hardening-suite.md) tranche
  remediates the 2026-06-10 security and posture assessment across discovery
  correctness, supply-chain gates, daemon panic-safety, secret egress, gateway
  trust, and docs compaction.

## Paused Strict Lane

[`g08.018`](./g08/018-apple-containers-native-backend-prototype.md) is paused
watch-only.
[`1051`](./g08/batch-cards/1051-establish-apple-runtime-and-stack-plan.md) is
complete. [`1052`](./g08/batch-cards/1052-add-apple-native-stack-lifecycle.md)
is complete with a passing four-service native lifecycle. [`1053`](./g08/batch-cards/1053-prove-apple-parity-and-decide-support.md)
is complete with a watch-only decision. No execution card is ready.

[`g08.017`](./g08/017-workspace-ssh-agent-mount-resilience.md) (workspace
SSH-agent mount resilience) is complete: `container up` and `doctor` both
pre-empt a stale colima SSH-agent socket with the `colima restart <profile>`
fix. `g08` stays open for the next scope.
[`g08.016`](./g08/016-suppression-hygiene-and-dead-code-precision.md)
(suppression hygiene + dead-code precision) is complete. The
[`g08.010`](./g08/010-security-and-posture-hardening-suite.md)
security and posture hardening suite is complete across all five milestones
([`g08.011`](./g08/011-discovery-and-doctor-correctness.md),
[`g08.012`](./g08/012-supply-chain-and-ci-security-gates.md),
[`g08.013`](./g08/013-daemon-panic-safety-and-secret-egress-hardening.md),
[`g08.014`](./g08/014-gateway-route-table-trust-model.md),
[`g08.015`](./g08/015-docs-spine-compaction.md)). `g08` remains the active
generation, open for further scope — no `g09` rollover is implied.

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

Keep `g08.018` paused until guide `077`'s boot-time discovery reassessment gate
is met.
No `g09` rollover at this time.

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
