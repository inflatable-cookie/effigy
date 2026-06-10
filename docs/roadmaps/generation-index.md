# Roadmap Generation Index

Current generation: g08
Updated: 2026-06-05

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

`g07` is closed.

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
- `g07.030` through `g07.034` completed a one-call graph exploration surface
  that targets whole-agent workflow cost rather than raw query latency.
- `g07.035` through `g07.045` reopen the graph lane for CodeGraph-parity work:
  benchmark harness, FTS-backed source evidence, traversal-aware explore
  assembly, richer language extractors, framework routes, source-section
  no-reread packets, affected-test workflow, scale hardening, agent adoption,
  and closeout.
- `g07.046` through `g07.049` completed the bounded CodeGraph parity
  follow-up lane for warm-query recovery and fixture-backed proof.
- `g07.050` through `g07.055` open the init setup-wizard lane so `effigy init`
  can expose one shared setup-job surface through TTY prompts, checklist JSON,
  and non-interactive action execution.
- `g07.056` through `g07.063` completed the bounded residual cleanup pass:
  codegraph emitter deduplication, query/manifest decomposition, init-module
  boundary cleanup, JSON/help convention cleanup, runner/test fixture cleanup,
  crate-boundary rejustification, and closeout proof.
- `g07.064` through `g07.071` completed the bounded
  residual-maintainability follow-up tranche: warning-only god-file reduction,
  stubborn duplicate-block follow-through, graph-test decomposition,
  script-command boundary reduction, runner-private helper convergence, and
  closeout proof.
- `g07.072` through `g07.078` completed the practical graph agent-adoption
  tranche:
  freshness trust, behavior-shaped query ranking, edit-target/test-target
  packets, cross-repo benchmark proof, and balanced skill/docs guidance.
- The suite explicitly excludes MCP, a graph daemon, external language plugins,
  JavaScript runtime dependencies, and LLM-generated summaries as canonical
  graph data.

`g08` is active. Milestones `g08.001` through `g08.009` are complete; the
`g08.010` security and posture hardening tranche is open.

- `g08.001` through `g08.008` completed the graph-aware scan intelligence
  generation:
  scan/graph readiness, additive enrichment for existing scans, graph-native
  scans for boundaries, likely dead code, and validation risk, plus agent
  guidance, JSON examples, benchmark proof, and closeout.
- `g08.009` owns the code-quality boundary sweep follow-up. Its completed
  batch cards cover command-surface descriptor convergence, Rhai feature
  descriptor convergence, container `up` phase boundary cleanup, Effigy
  repo-marker/root-rule convergence, selected duplicate-block reduction,
  boundary/dead-code scan self-adoption, dead-code Rust signal repair, residual
  false-positive precision, and the final burn-down to 0 dead-code findings.
- `g08.010` opens the 2026-06-10 security and posture hardening tranche from the
  architecture assessment: `g08.011` discovery/doctor correctness,
  `g08.012` supply-chain and CI security gates, `g08.013` daemon panic-safety
  and secret egress hardening, `g08.014` gateway route-table trust model, and
  `g08.015` docs spine compaction.

`g08.011` (Discovery and Doctor Correctness) is complete: fixture manifests are
excluded from ambient discovery via `[catalog.discovery] ignore`, the doctor
schema validator now recognizes `[catalog.discovery]`, and `effigy doctor`
reports `err:0` on this repo.

`g08.012` (Supply-Chain and CI Security Gates) is complete through Batches A+B:
`deny.toml` policy authored and `cargo deny check` green locally (advisories,
bans, licenses, sources all ok). Batch C (CI workflow wiring + dependabot) is
held on explicit human workflow-edit approval.

`g08.013` (Daemon Panic-Safety and Secret Egress Hardening) is complete: panic
audit + lock-poison conversion across the gateway and process supervisor,
documented invariants on all eight proxy builder sites, and a redacting
`SecretValue::Serialize` with the vault as the sole explicit-exposure path.

`g08.014` (Gateway Route-Table Trust Model) is complete: contract `033`
promoted; read-path integrity gate (owner-only `0o600` + managed marker; daemon
fails closed keeping last-known-good); and trust state surfaced in `effigy
gateway status` and `effigy doctor`.

`g08.012` (Supply-Chain and CI Security Gates) is complete: the `cargo deny`
policy, the CI `supply-chain` job (workflow-edit approval granted 2026-06-10),
and Dependabot weekly updates are all in place.

The g08.010 security and posture hardening tranche is **complete** through
`g08.015`: discovery/doctor correctness, supply-chain + CI gates, daemon
panic-safety + secret egress, gateway route-table trust, and docs-spine
compaction (656 logs archived, logs index 677 → 21 entries).

`g08` remains the **active** generation — open for further scope. No `g09`
rollover is implied by closing this tranche.

`g08.016` (Suppression Hygiene and Dead-Code Precision) is planned from the
2026-06-10 post-hardening scan sweep: consolidate the workspace clippy allows
into `[workspace.lints]`, clear residual suppressions and one dead function, and
fix the dead-code scanner's `use`-edge/test-entrypoint false positives.

Active ready card: `g08.016` Batch A (workspace lint consolidation; no workflow
edit). Batch C's CI-flag retirement is approval-gated.

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

No current dead-code residual batch remains.
