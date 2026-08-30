# Roadmap Generation Index

Current generation: g08
Updated: 2026-08-29

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

`g08` is active. Milestones `g08.001` through `g08.031` are complete.
`g08.027` completes papercut portfolio discovery and capture under strict spec
`100`; cards `1070` and `1071` are complete.

`g08.028` is complete under strict spec `101`. It replaces ambient descendant
catalog discovery with explicit root-owned membership; cards `1072` through
`1075` are complete.

`g08.029` is complete under strict spec `102` and contract `038`. Card `1076`
makes `[test]` the sole v0.11 test authority and hardens `--plan` as a
no-execution boundary.

`g08.030` is complete under strict spec `103` and contract `039`. Card `1077`
requires a successful manually dispatched hosted CI run for the exact
candidate source commit before release preparation or execution.

`g08.031` is complete under archived strict spec `104` and contract `040`.
Cards `1078` through `1081` delivered the Bun pin planner, command surface,
interlocks, disposable consumer proof, public guidance, and pin-only text-lock
fallback for Bun process-enumeration failures.

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

`g08.016` (Suppression Hygiene and Dead-Code Precision) is **complete**: clippy
allows consolidated into `[workspace.lints]` (33 per-site allows removed; plain
`cargo clippy` matches CI), one dead function removed, suppression floor 44 → 11,
and the dead-code scanner now refuses a stale index instead of emitting false
positives.

`g08.017` (Workspace SSH-Agent Mount Resilience) is **complete**: a VM-side
preflight detects a stale/absent colima-forwarded SSH-agent socket, and both
`effigy container up` and `effigy doctor` pre-empt it with the
`colima restart <profile>` remediation (instead of the cryptic nerdctl
`mkdir ... file exists`); the cause/recovery is documented.

`g08.018` through `g08.023` complete the local dependency management suite:
the shared `effigy deps` foundation, Cargo links, Bun links, doctor/hygiene,
portfolio proof, and operator closeout.

Cards `1051` through `1087` and strict specs `099` through `107` are complete.

`g08.033` restored doctor parity with the contract-032 secret surface and
proved the corrected installed CLI against the operator-selected Bovine
consumer.

`g08.034` is complete under archived strict spec `107`. Cards `1086` and
`1087` audited the whole current public behavior surface against active user,
agent, built-in, and generated docs, repaired verified gaps, added proportional
recurrence guards, and closed with full evidence.

`g08.035` is active under strict spec `108` and contract `041`. Cards `1088`
through `1090` cover the generic profile/structural foundation, bounded docs
context query, and generic plus Northstar adoption proof. Card `1088` is
complete; card `1089` is ready.

## Strategic runway (Atlas 2026-08-17)

The operator selected the agent-native maintainer theme on 2026-08-29 as the
narrow `g08.035` documentation-graph extension. No `g09` rollover is implied.

Canonical atlas artifact:
[`docs/vision/020-strategic-runway-atlas-v1.md`](../vision/020-strategic-runway-atlas-v1.md)

Candidate themes and selection state:
[`docs/roadmaps/backlog/g09-candidate-themes.md`](./backlog/g09-candidate-themes.md)

Horizon summary:

1. **Stabilize and choose** — refresh instruction surfaces; operator picks one
   lane owner.
2. **Governed operations** — populate vision governance registers and run
   reviews on live data.
3. **Agent-native maintainer experience** — graph/scan/papercuts adoption with
   measured proof.
4. **Platform scale** — portfolio orchestration, distribution maturity, promoted
   research Phase 3 only after B–C discipline.

Strict spec `108` and ready card `1089` continue the selected intent. Do not open
`g09` or start release work from this selection.

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

Execute ready card
[`1089`](./g08/batch-cards/1089-add-bounded-documentation-context-query.md).
The second governance review remains due by 2026-09-17. No release action or
`g09` rollover is implied.
