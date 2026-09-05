# 120 Docs Context Latency and Freshness Strict Lane

Status: Active
Owner: Effigy orchestrator
Created: 2026-09-05
Roadmap: [`g09.005`](../roadmaps/g09/005-docs-context-latency-and-freshness.md)
Ready card: [`1113`](../roadmaps/g09/batch-cards/1113-docs-context-latency-and-freshness.md)
Contract: [`041`](../contracts/041-documentation-graph-profile-contract.md)
Architecture: [`024`](../architecture/024-repository-defined-documentation-graph.md)
Guides: [`079`](../guides/079-documentation-graph-profiles-and-context.md),
[`076`](../guides/076-code-graph-and-agent-workflows.md)

## Outcome

`effigy docs context` on a current index answers inside an agent-sized budget
on a repository the size of Effigy, an incremental docs-only refresh completes
inside the default budget, and a timeout says what it was doing. Contract
`041` semantics are unchanged.

## Fixed Decisions

- Measure before changing. The first deliverable is a reproduced table for
  cold, stale-incremental, and warm queries with binary version, source SHA,
  repository SHA, dirty state, corpus size, budget, elapsed wall/user/sys, and
  outcome, on the Effigy repository and the `generic-handbook` fixture, with
  no other graph process running. The roadmap's Chatterbox baseline is a
  pointer, not the lane's evidence.
- Frozen budgets (Effigy repository, `--max-sections 3 --max-bytes 6000`,
  release-profile binary, no concurrent graph process):
  - **Warm:** with `graph status` reporting 0 stale paths, the query succeeds
    under `EFFIGY_GRAPH_TIMEOUT_MS=5000`; record p50 over five runs, target
    ≤ 2000 ms.
  - **Stale-incremental:** after editing ≤ 50 tracked Markdown files and no
    other change, one query under the default 120000 ms budget completes the
    refresh and returns results; record elapsed, target ≤ 30 s. A second
    query immediately after succeeds under 5000 ms.
  - **Cold:** explicit `effigy graph index` remains unbounded; record elapsed
    and do not regress it by more than 10 % versus the lane's own baseline.
  - **Fixture:** every `generic-handbook` benchmark case succeeds under
    `EFFIGY_GRAPH_TIMEOUT_MS=5000` warm.
- Repairs are allowed only where the reproduced table shows the budget is
  missed, and must stay inside the existing refresh path, refresh lock, health
  snapshot, and typed timeout. Candidate areas the evidence may point at:
  freshness-scan cost on a current index, whole-corpus `graph_search` rebuild
  on any change, per-row inserts outside a transaction, and detached-worker
  behaviour after timeout. None is prescribed.
- Timeout diagnostics: keep `effigy.graph.timeout.v1`; additive fields may
  name the phase in progress and files processed so far. No schema id bump.
- Not allowed: a second index or refresh model, daemon, background service,
  embeddings, new public flag or environment variable, changing default
  budgets, changing ranking or budget rules of contract `041`, editing the
  benchmark matrix, or touching any repository other than Effigy and the
  fixture.

## Dependency Runway

```text
spec 119 / card 1112 merged
  -> 1113 reproduce, freeze table, repair to budget, replay pilot questions locally
  -> exact-head review and merge
  -> Chatterbox decides whether g09.006 cross-repository routing compiles
```

One worker owns card `1113`. Use a frontier-capable implementation worker:
the lane needs disciplined measurement and root-cause work in the graph
crate. Material review remains with the orchestrator.

## Whole-Lane Review Oracle

Reject the lane if any counterexample survives:

1. A code change lands without a prior reproduced measurement table that
   names the missed budget it addresses.
2. Any measurement lacks binary version, source SHA, repository SHA, dirty
   state, corpus size, budget, and elapsed wall/user/sys, or was taken while
   another graph process held or contended for the refresh lock.
3. A warm query on a current Effigy index still fails under 5000 ms, or the
   stale-incremental case still fails under the default budget.
4. `perf:docs-context-benchmark` fails, or its frozen matrix changed.
5. A result loses exact path/span/source, freshness identity, or the
   unknown-authority/currentness defaults; or the refresh lock, health
   snapshot, or bounded-failure envelope is bypassed.
6. A second index, refresh model, daemon, embedding, new flag, or default
   budget change appears; or a timeout is "fixed" by raising a default.
7. The pilot's local replay (K4 `catalog_tasks` field, K5 release execute
   publication) returns a fabricated or wrong-repository source, or a
   no-match control returns anything.
8. Cold `graph index` regresses beyond 10 % of the lane's own baseline.

Smallest counterexample set: one warm query at 5000 ms on Effigy; one
≤ 50-file Markdown edit followed by one default-budget query and one 5000 ms
query; one fixture benchmark run; one no-match query; one cold index timing.

## Validation And Evidence

Card `1113` maps every oracle row to named proof. Use release-profile
binaries for timings, run each timing at least three times, and record the
absence of concurrent graph processes. Run `effigy perf:docs-context-benchmark`,
focused codegraph tests, `effigy qa`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, and `git diff --check`. Write
one dated evidence log with the before/after tables.

## Stop Conditions

Stop and return to the orchestrator if the budget cannot be met without a
second index, a daemon, an engine rewrite, a contract `041` semantics change,
a default-budget change, or edits outside Effigy and the fixture; if the
reproduced table contradicts the roadmap baseline in a way that changes the
target; or if the fix needs a schema id bump.

## Next Task

Execute card `1113`.
