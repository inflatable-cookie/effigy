# 1113 - Docs Context Latency and Freshness

Roadmap: [`../005-docs-context-latency-and-freshness.md`](../005-docs-context-latency-and-freshness.md)
Spec: [`../../../specs/archive/120-docs-context-latency-and-freshness-strict-lane.md`](../../../specs/archive/120-docs-context-latency-and-freshness-strict-lane.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)

Status: Complete
Owner: graph freshness scan, lazy refresh, search-index rebuild, docs-context
query path, graph time-budget diagnostics
Created: 2026-09-05
Queued since: 2026-09-05 operator-confirmed direction relayed by the Northstar
Chatterbox; serial prerequisite `1112` merged at `f1732c87`
Completed: 2026-09-05; PR `91` merged at `c111f883`

## Purpose

Make `effigy docs context` answer inside an agent-sized budget on a current
index and finish an incremental docs-only refresh inside the default budget,
with the repair chosen from reproduced evidence.

## Work

1. **Reproduce.** Build a release-profile binary from the lane's base SHA.
   Confirm no other graph process is running. On the Effigy repository and
   the `generic-handbook` fixture, record cold (`graph index`), stale-
   incremental (edit ≤ 50 tracked Markdown files, then one default-budget
   query, then one 5000 ms query), and warm (current index, 5000 ms, five
   runs) with every identity field spec `120` names. Record the same for the
   installed `v0.12.1+local.aafbd93` binary once, to confirm parity.
2. **Diagnose.** Attribute the warm cost and the stale-refresh cost to
   phases (freshness scan, lock wait, rebuild walk, `graph_search` rebuild,
   query, render). Add additive phase/progress fields to the timeout detail
   if that is what makes the attribution visible.
3. **Repair to budget.** Change only what the attribution shows, inside the
   existing refresh path and lock. Keep contract `041` ranking, budgets,
   provenance, and freshness identity unchanged.
4. **Replay.** Run `perf:docs-context-benchmark` (matrix unchanged; append a
   freeze-history line naming the commit). Replay the pilot's K4
   (`catalog_tasks` JSON field) and K5 (release execute publication) questions
   against Effigy only, plus one no-match control, all under 5000 ms warm.
5. **Close.** Update guides `079` and `076` for any changed diagnostic
   fields; add `CHANGELOG.md` `[Unreleased]` entries; write one evidence log
   with before/after tables.

## Acceptance

- [ ] reproduced before-table on Effigy and fixture with all identity fields,
      taken with no concurrent graph process
- [ ] warm Effigy query succeeds under `EFFIGY_GRAPH_TIMEOUT_MS=5000`; p50 of
      five runs recorded (target ≤ 2000 ms)
- [ ] stale-incremental (≤ 50 Markdown files) completes under the default
      budget (target ≤ 30 s) and the next 5000 ms query succeeds
- [ ] every fixture benchmark case succeeds warm under 5000 ms; matrix unchanged
- [ ] cold `graph index` within 10 % of the lane's own baseline
- [ ] timeout detail keeps schema `effigy.graph.timeout.v1`; any new fields
      are additive and documented
- [ ] K4 and K5 local replays return no fabricated or wrong-repository
      source and the no-match control returns an empty report (spec `120`
      oracle row 7). Known limitation recorded 2026-09-05 by Chatterbox
      ruling: K4 misses guide `026` because exact snake_case identifiers are
      tokenised into common words (a contract `041` retrieval defect outside
      this lane's scope, which forbids ranking changes); K5's expected
      "GitHub Release" phrase is absent from guide `051`, so that expectation
      needs inference and is a pilot expectation defect. Both are re-planned
      in triage `20260905-docs-context-identifier-retrieval-and-k5-expectation`.
- [ ] no second index, daemon, new flag/env var, default-budget change, or
      contract `041` semantic change

## Review Oracle

Falsify these counterexamples before PR creation:

1. A code diff exists with no before-table naming the budget it addresses.
2. A timing row lacks an identity field or was taken under lock contention.
3. Warm Effigy query still times out at 5000 ms after the repair.
4. Stale-incremental still times out at the default budget, or the following
   5000 ms query fails.
5. Benchmark red, or its matrix edited.
6. Provenance, freshness identity, lock, health snapshot, or bounded-failure
   envelope changed or bypassed.
7. A default budget was raised, or a new index/daemon/flag appeared.
8. K4/K5 replay returns a wrong or fabricated source; no-match returns a result.

## Validation

- focused codegraph tests for changed refresh, scan, search-index, and
  timeout-detail behaviour
- `effigy perf:docs-context-benchmark`
- `effigy graph affected` for changed source, then direct targets
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

One dated closeout log under `docs/logs/2026-09/` with the before and after
measurement tables (every identity field), phase attribution, the diff
summary, benchmark output, the K4/K5/no-match replay rows, and validation.

## Stop Conditions

Stop if meeting the budget needs a second index, daemon, engine rewrite,
contract `041` semantics change, default-budget change, schema id bump, or
edits outside Effigy and the fixture; or if the reproduced table changes the
target enough that the frozen budgets need re-planning.

## Next Task

PR `91` was opened at exact reviewed head `d8b9b36d` and merged at `c111f883`.
K4/K5 retrieval limitations are recorded in the canonical triage note.
