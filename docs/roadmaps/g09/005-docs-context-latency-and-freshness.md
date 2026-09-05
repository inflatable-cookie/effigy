# g09.005 Docs Context Latency and Freshness

Status: Queued (serial after `g09.004`)
Created: 2026-09-05
Spec: [`120`](../../specs/120-docs-context-latency-and-freshness-strict-lane.md)
Card: [`1113`](./batch-cards/1113-docs-context-latency-and-freshness.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Architecture: [`024`](../../architecture/024-repository-defined-documentation-graph.md)
Guides: [`079`](../../guides/079-documentation-graph-profiles-and-context.md),
[`076`](../../guides/076-code-graph-and-agent-workflows.md)
Origin: Northstar shared-knowledge retrieval pilot,
`northstar/docs/triage/20260905-093742-shared-knowledge-retrieval-pilot.md`
(Northstar `main` through `4ce522b`); operator-confirmed direction relayed by
the Northstar Chatterbox on 2026-09-05

## Purpose

Make repository-local exact-section docs retrieval usable inside an
agent-sized latency budget, with reproduced evidence before any repair, and
without changing what contract `041` promises about provenance, freshness,
authority, or bounded failure.

## Why Now

The operator prioritised cross-agent knowledge sharing over caching and
approved a small pilot. The pilot froze five historical questions across
Northstar, Effigy, and Underlay and found every one answerable from local
sources, but every `effigy docs context` probe under a 5000 ms budget timed
out during stale refresh, and one Northstar probe timed out at the default
120000 ms. No warm result was observed and no root cause was named. Effigy
owns retrieval; Northstar owns documentation authority and lifecycle doctrine.
This is the first, bounded step; cross-repository routing is conditional on
its evidence (see [`g09.006`](./006-cross-repository-source-routing.md)).

## Reproduced Baseline (2026-09-05, Effigy checkout)

Recorded by Chatterbox before promotion; the lane must re-measure under its
own controlled conditions. Identity: installed binary `v0.12.1+local.aafbd93`;
source `main` at `af2a96ea9`; no commits touched `crates/effigy-codegraph`,
docs-policy, or the graph/docs runner paths between `aafbd93` and `af2a96ea9`,
so the installed binary and current source are the same for this surface.
Corpus: 3946 indexed files, 2468 tracked Markdown files, graph DB ~240 MB.
A concurrent `effigy graph index` from another agent ran during several of
these measurements, so absolute numbers are upper bounds, not clean figures.

| Condition | Command | Result |
| --- | --- | --- |
| Stale index, 5000 ms budget, twice in a row | `docs context "catalog_tasks" --max-sections 3 --max-bytes 6000` | both timed out; second run was still stale, so a short budget makes no persistent progress |
| Stale index, default 120000 ms budget | same | timed out at 120.0 s (36.6 s user) |
| Explicit `effigy graph index` | `graph index --json` | completed in 647 s wall (169 s user, 292 s sys) under contention |
| Current index (`graph status`: 0 stale paths), unbounded | same query | **succeeded in 10.7 s** (2.9 s user); three results with provenance |
| Current index, 5000 ms budget, immediately after | same query | timed out |

Observations for the lane, not conclusions: a warm query on a current index
costs about 10 s here, which alone exceeds the pilot's 5000 ms budget; the
lazy refresh path rebuilds the whole `graph_search` table whenever any file
changed (`refresh_search_index`, called from `run_index_unlocked`), which is
proportional to the corpus rather than to the change; and the time budget
detaches rather than cancels the worker, so a timed-out refresh may still be
running under the lock when the next query arrives.

## Decision

- Reproduce first: cold, stale-incremental, and warm behaviour with explicit
  version, repository, and source identity, using the existing
  `perf:docs-context-benchmark` corpus where it fits and the pilot's five
  frozen questions replayed against Effigy's own repository.
- Freeze budgets in spec `120` before repair. Any repair follows reproduced
  evidence and stays inside the existing refresh path, lock, health snapshot,
  and typed timeout contract. No engine rewrite, no second index, no daemon,
  no blanket timeout increase.
- Retain and, where the evidence shows a gap, improve timeout diagnostics
  (what phase was running, how much was done) without changing the
  `effigy.graph.timeout.v1` schema id.

## Cards

- [ ] [`1113`](./batch-cards/1113-docs-context-latency-and-freshness.md) —
  queued; becomes ready when card `1112` merges

## Acceptance

- reproduced measurements for cold, stale-incremental, and warm queries with
  identities recorded, before any code change
- after repair, the frozen budgets in spec `120` hold on the Effigy repository
  and the benchmark fixture, measured and recorded
- `perf:docs-context-benchmark` stays green with an unchanged frozen matrix
- provenance, freshness identity, locking, unknown authority/currentness, and
  the bounded-failure contract are unchanged
- the pilot's five questions replayed against Effigy-local sources return the
  expected Effigy sources where they exist locally (K4, K5) and report no
  match, not fabricated results, where they do not

## Non-Goals

- cross-repository routing, portfolio directories, or any `--repo` fan-out
- embeddings, MCP server, hosted service, separate index or daemon
- artifact caching, agent notification changes
- release or workflow mutation, consumer-repository writes
- raising every caller's timeout as the product answer

## Dispatch Manifest

Published for the coordinator at the promoting commit on `main`.

- **Lane:** card `1113`, roadmap `g09.005`, strict spec `120`. State:
  queued. **Serial edge:** starts only after card `1112` (`g09.004`) has
  merged to `main`. Not approved for parallel execution with `1112`.
- **Prerequisites:** `1112` merged; clean `main`; no other active strict
  lane. **Completion:** PR merged with the evidence log, card, roadmap, spec,
  guide, and changelog closed out, and a measured statement of the warm and
  stale budgets on Effigy and the fixture.
- **Owned mutable paths:** `crates/effigy-codegraph/src/**`,
  `src/runner/graph_time_budget.rs`, `src/runner/docs_command/**` and
  `src/runner/graph_command/**` where present, `scripts/benchmark-docs-context.rhai`
  (freeze-history append only; matrix unchanged), tests under those crates
  and `src/tests/**`, `docs/guides/079-documentation-graph-profiles-and-context.md`,
  `docs/guides/076-code-graph-and-agent-workflows.md`.
  **Reserved shared closeout surfaces:** `CHANGELOG.md` `[Unreleased]`,
  `docs/logs/2026-09/`, `docs/logs/README.md`, this roadmap, card `1113`,
  spec `120`, contract `041` (Next Task only unless a drift trigger fires),
  `docs/specs/README.md`, `docs/roadmaps/README.md`, `docs/roadmaps/g09/README.md`.
- **Concurrency:** no approved siblings. Serial after `1112`.
- **Worker capability class:** frontier-capable implementation worker; this
  lane requires measurement discipline and root-cause work in the graph
  crate, not a mechanical edit.
- **Acceptance evidence and review oracle:** card `1113` acceptance and spec
  `120` whole-lane oracle; benchmark run, fixture and Effigy measurements
  before/after, `effigy qa`, fmt, clippy, `git diff --check`; one dated
  evidence log.
- **Stop conditions and escalation owner:** spec `120` stop conditions.
  Planning questions escalate to the coordinator, then Chatterbox. Any
  change to contract `041` semantics escalates to Chatterbox before code.

## Next Task

Wait for `1112` to merge, then execute card `1113`. On closeout, Chatterbox
decides whether the evidence unlocks
[`g09.006`](./006-cross-repository-source-routing.md).
