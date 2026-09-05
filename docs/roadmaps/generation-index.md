# Roadmap Generation Index

Current generation: g09
Updated: 2026-09-05 (compacted)

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

One paragraph per generation; milestone-level history lives in each
generation's `README.md`, its roadmap files, and the archived logs.

- `g05` (closed, `g05.001`–`g05.027`): secret and local configuration
  management, post-release ownership cleanup, schema-shape consolidation, and
  reusable-core hardening.
- `g06` (closed, `g06.001`–`g06.008`): post-`v0.7.0` codebase lean-down:
  god-file reduction, fixture convergence, CLI/help deduplication, typed
  contract-shape reuse, compatibility-branch deletion, runner-private
  domain-logic reduction.
- `g07` (closed, `g07.001`–`g07.078`): native code graph intelligence,
  incremental indexing and watch mode, ranking quality, CodeGraph parity,
  init setup wizard, bounded leanness cleanup, and graph agent-adoption
  follow-through.
- `g08` (closed, `g08.001`–`g08.048`): graph-aware scan intelligence,
  code-quality boundary sweep, security and posture hardening, local
  dependency management, release integrity and patch-lane hardening,
  papercuts discovery, explicit catalog membership, unified test
  orchestration, pre-release CI proof, Bun pinning, vision governance,
  documentation coverage and parity, the repository-defined documentation
  graph, external skill task runner, help-first discovery, and catalog-pack
  acquisition, publication, and cutover.
- `g09` (active): operator and consumer contract clarity. `g09.001` shipped
  and `g09.002` rolled back executable command namespaces; `g09.003` replayed
  the consumer contract against a frozen Acowtancy and published the first
  comparison scorecard; `g09.004` made failed release gates diagnosable;
  `g09.005` brought warm `docs context` from ~10 s to ~600 ms; `g09.007`
  fixed exact-identifier retrieval; `g09.006` added opt-in cross-repository
  source routing. All seven are complete. Open decisions: the consumer
  release-prepare keep-on-failure question (consumer maturity was settled on
2026-09-05: `007` section 6 adoption posture) and the
  release-prepare keep-on-failure question, both in `docs/triage/`.

## Rollover history

- `g01` → `g02`, `g02` → `g03`, `g03` → `g04`, `g04` → `g05`, `g05` → `g06`,
  `g06` → `g07`, `g07` → `g08`: each closed by explicit operator rollover
  after the front doors agreed the old generation was no longer the live queue.
- `g08` → `g09`: opened 2026-09-02 after the Northstar Atlas refresh
  ([`../vision/020-strategic-runway-atlas-v1.md`](../vision/020-strategic-runway-atlas-v1.md))
  and the operator's Theme 4 selection.
