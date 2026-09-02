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
- `g08/` current graph-aware scan, hardening, and local dependency management
  generation
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
  trust, and docs compaction. Milestones `g08.016` and `g08.017` are also
  complete. The completed
  [`g08.018`](./g08/018-local-dependency-management-suite.md) suite added the
  shared `effigy deps` foundation, Cargo and Bun local links, doctor hygiene,
  and portfolio proof through `g08.023`. The bounded
  [`g08.024`](./g08/024-initial-current-version-release-tag.md) follow-up lets
  an explicitly configured new repository tag its already-declared first
  version without weakening later monotonic releases.
  [`g08.025`](./g08/025-annotated-release-tag-integrity.md) now repairs the
  release tag object boundary exposed by a real Swallowtail candidate.
  The completed [`g08.026`](./g08/026-patch-release-lane-hardening.md) lane removes
  a persistent loopback test-state leak, settles prepared-source drift policy,
  and proves the `0.9.1` candidate.
  The completed [`g08.027`](./g08/027-papercuts-discovery-and-capture.md) lane adds
  rootless project/portfolio papercut discovery and safe single-project capture.
  The completed [`g08.028`](./g08/028-explicit-catalog-membership.md) lane replaces
  ambient descendant discovery with root-owned catalog membership under
  contract `037`.
  Completed [`g08.029`](./g08/029-unified-test-orchestration-v011.md) makes
  `[test]` the sole v0.11 test authority under contract `038`.
  Completed [`g08.030`](./g08/030-pre-release-ci-proof.md) requires a green
  hosted CI run for the exact candidate source SHA before release work under
  contract `039`.
  Completed [`g08.031`](./g08/031-bun-committed-dependency-pinning.md) delivers
  the separate committed Bun override workflow and pin-only text-lockfile
  fallback under contract `040`; cards `1078` through `1081` are complete.
  Completed [`g08.032`](./g08/032-vision-governance-operationalization.md)
  operationalizes vision governance registers and the first review cycle under
  archived strict spec `105`; cards `1082` through `1084` are complete.
  Completed [`g08.034`](./g08/034-documentation-coverage-parity.md) audits
  current public behavior against active user, agent, built-in, and generated
  docs under archived strict spec `107`; cards `1086` and `1087` are complete.
  Completed [`g08.035`](./g08/035-repository-defined-documentation-graph.md)
  implements repository-owned documentation graph profiles and bounded context
  retrieval under archived strict spec `108`; cards `1088`, `1089`, and `1090`
  are complete.
  Completed [`g08.036`](./g08/036-documentation-instruction-and-help-parity-refresh.md)
  refreshed scan, agent-instruction, feature-documentation, generated-reference,
  and shipped-help parity under archived strict spec `109`; card `1091` is
  complete.
  Completed [`g08.037`](./g08/037-external-skill-task-runner.md) adds explicit
  installed-skill task execution with the consuming repository retained as the
  runtime target; card `1092` is complete and strict spec `110` is archived.
  Completed [`g08.038`](./g08/038-help-first-command-discovery.md) groups
  command discovery under the six operator-job topics without adding execution
  aliases; card `1093` is complete and strict spec `111` is archived.
  Completed [`g08.039`](./g08/039-rhai-profile-independent-limits-papercut.md)
  makes Rhai expression-depth parsing profile-independent while preserving
  release limits; card `1094` is complete and strict spec `112` is archived.
  Completed [`g08.040`](./g08/040-catalog-pack-acquisition-prototype.md) proves
  explicit installed catalog-pack acquisition and recovery while retaining the
  permanent compiled baseline; card `1095` is complete and strict spec `113` is
  archived.
  Completed [`g08.041`](./g08/041-catalog-fragment-listing-papercut.md) corrected
  bundled fragment inventory so `service list` reports only `service.toml`
  parents while filesystem/pack directory listing stays unchanged; card `1096`
  is complete.
  Completed [`g08.042`](./g08/042-markdown-frontmatter-extraction-papercut.md)
  keeps leading YAML frontmatter out of Markdown section inventory while
  preserving metadata and exact spans; card `1097` is complete.
  Completed [`g08.043`](./g08/043-docs-context-no-match-benchmark-isolation-papercut.md)
  isolates empty-result benchmark proof from Effigy's live documentation
  corpus; card `1098` is complete.
  Completed [`g08.044`](./g08/044-rhai-storage-create-only.md) adds atomic
  create-if-absent behavior to the retained Rhai storage surface; card `1099`
  is complete and strict spec `114` is archived.
  Completed [`g08.045`](./g08/045-child-catalog-suite-registry-papercut.md)
  preserves an ancestor container registry during child-catalog suite task-ref
  expansion; card `1100` is complete under contract `038`.
  Completed [`g08.046`](./g08/046-docs-context-time-budget-papercut.md) shares
  the graph time budget and progress boundary with cold docs-context refresh;
  card `1101` is complete under contract `041`.
  Completed [`g08.047`](./g08/047-docs-context-traversal-budget-papercut.md)
  reserves bounded result capacity for typed-relation traversal; card `1102`
  is complete under contract `041`.
  Active [`g08.048`](./g08/048-catalog-pack-publication-and-cutover.md)
  owns the official catalog-pack publication and generated-baseline cutover
  under strict spec `115`; card `1103` is complete.

## Active Strict Lane

Spec `115` is active for `g08.048`. Cards `1103` through `1106` are complete;
the `1106` generated-baseline cutover PR is under review. Cards `1107` and
`1108` stay blocked until the orchestrator refreshes their readiness after
that merge.

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

Card `1106`'s generated-baseline cutover PR is under review. After it merges,
refresh ready-frontier status for cards `1107` and `1108`. Keep Effigy release
execution, S3 extraction, and `g09` rollover behind their named gates.

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
