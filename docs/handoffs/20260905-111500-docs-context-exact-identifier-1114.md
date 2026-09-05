---
title: Docs context exact identifier retrieval worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: docs-context query terms, lexical seeding and scoring, benchmark matrix
created: 2026-09-05
updated: 2026-09-05
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260905-111500-docs-context-exact-identifier-1114.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, g09.007, 1114]
---

## What This Thread Was Doing

The coordinator is dispatching the single approved implementation lane for
exact identifier retrieval. Card `1114` owns the bounded implementation,
tests, benchmark freeze, evidence, validation, and a reviewable PR.

This dispatches one bounded implementation lane. No transcript or second prompt
is part of the authority chain.

## Why It Matters

Agents who type an exact identifier such as `catalog_tasks` should retrieve the
section that literally contains it without weakening the existing FTS recall,
ranking, freshness, provenance, or budget contracts.

## Current State

- **Repository:** `effigy` at `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `0c412682f1f7e01a220709dab6ab197d184d39a9`
- **Pushed main verification:** clean `main`, `HEAD == origin/main`
- **Planning checkout:** clean
- **Worker mode:** implementation worker dispatched by the orchestrator; this handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** roadmap `g09.007`, card `1114`, spec `121`; specs `119` and `120` archived; g09.006 remains planned-only
- **Worker branch:** `worker/g09-007-docs-context-exact-identifier-1114`
- **Worker worktree:** `/Users/tom/.paseo/worktrees/310mya31/g09-007-docs-context-exact-identifier-1114`
- **Worktree creation command:** Paseo `create_workspace`; `isolation: worktree`, `mode: branch-off`, `baseBranch: origin/main`
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree first, named/manual fallback only when required.
- **Required sibling worktree links:** `none`
- **Active spec lane:** [`docs/specs/121-docs-context-exact-identifier-retrieval-strict-lane.md`](../specs/121-docs-context-exact-identifier-retrieval-strict-lane.md)
- **Roadmap milestone:** [`docs/roadmaps/g09/007-docs-context-exact-identifier-retrieval.md`](../roadmaps/g09/007-docs-context-exact-identifier-retrieval.md)
- **Ready cards, in order:** [`docs/roadmaps/g09/batch-cards/1114-docs-context-exact-identifier-retrieval.md`](../roadmaps/g09/batch-cards/1114-docs-context-exact-identifier-retrieval.md)
- **Allowed runway:** execute card `1114` only
- **Remaining card budget:** one card, one PR
- **Coordinator agent ID:** `0accca7b-4f0e-428a-b62c-b8755b32cc1c`
- **Delivery route:** coordinator-attached child with `notifyOnFinish: true`; the coordinator records scoped creation and returned child/workspace identity.
- **Dispatch topology:** sole approved ready-frontier lane
- **Parallel safety check:** no approved siblings; no serial edges; no shared mutable scope
- **Surfaces this lane owns:** `crates/effigy-codegraph/src/docs_context/**`, `crates/effigy-codegraph/src/storage.rs` search-query helpers only, `scripts/benchmark-docs-context.rhai` new cases plus freeze history, one generic fixture identifier document if needed, tests under those crates and `src/tests/**`, and guide `079`
- **Integration ownership:** coordinator owns `CHANGELOG.md` `[Unreleased]`, `docs/logs/2026-09/`, `docs/logs/README.md`, this roadmap, card `1114`, spec `121`, contract `041` only if a drift trigger fires, and planning front doors
- **Merge ordering:** same-repository PRs merge one at a time; the orchestrator refreshes this head against current `main` and re-reviews it if a sibling lane merges first
- **Canonical refs:** architecture `docs/architecture/024-repository-defined-documentation-graph.md`; contract `docs/contracts/041-documentation-graph-profile-contract.md`; guide `docs/guides/079-documentation-graph-profiles-and-context.md`
- **Review oracle:** card `1114` Review Oracle and spec `121` Whole-Lane Review Oracle
- **Model capability profile:** economical non-frontier day-to-day implementation worker
- **Worker provider/model identity:** `cursor/default` (adapter-selected runtime identity recorded by coordinator)
- **Frontier-worker justification:** `none`
- **Tool/runtime restrictions:** no FTS tokenizer or storage change, second index, stemming, fuzzy/prefix matching, synonyms, schema ID, budget, freshness, lock, traversal, currentness, authority, or existing benchmark-rank change; no workflow edits
- **Required validation:** focused docs-context tests; `effigy perf:docs-context-benchmark`; one warm Effigy timing at 5000 ms; `effigy graph affected`; `effigy qa`; fmt; clippy `-D warnings`; `git diff --check`
- **PR base/head:** current pushed `main` at `0c412682`; head pending
- **PR URL:** pending
- **Review state:** awaiting implementation and PR
- **Merge path:** orchestrator after accepted independent exact-head review and passing required checks

## Boundaries

- **In scope:** retain identifier-shaped raw terms containing `_`, `-`, `.`, `::`, or `/` between alphanumeric runs alongside split words; rank exact whole-term containment in section text, heading, path, or fields above split-word density; emit a match reason naming the exact term; preserve FTS `source_search` candidate recall; add and freeze the two specified benchmark cases; prove `graph`/`graphql` and identifier-prefix boundaries; preserve the spec 120 warm budget.
- **Out of scope:** FTS tokenizer/storage changes, second index, stemming, fuzzy/prefix matching, synonyms, embeddings, schema IDs, budget/freshness/lock/traversal/currentness/authority changes, moving existing benchmark ranks, g09.006, or planning.
- **Outcome shape:** smallest contract-valid implementation with tests, benchmark evidence, validation, and a reviewable PR. Do not merge.
- Do not invent architecture, change contracts, widen the roadmap, or decide unresolved retrieval semantics.
- Write only inside **Surfaces this lane owns**. Leave closeout/front-door surfaces assigned to **Integration ownership** to the coordinator.

## Important Context

- **Planning lineage:** K4 identifier-tokenisation defect from the completed g09.005 lane -> Chatterbox promotion -> g09.007 / spec 121 / card 1114.
- **Why this card is ready:** operator-confirmed direction froze the exact-term behavior, benchmark cases, boundaries, non-goals, oracle, and validation.
- **Decisions and preferences:** exact whole terms are additive alongside split words; only literal containment counts; candidate recall remains existing FTS; all eleven previous benchmark ranks remain unchanged; warm 5000 ms budget remains required.
- **Open tensions:** stop and escalate if any fix requires contract 041 semantics beyond identifier rule 2, changes existing benchmark ranks, or needs tokenizer/storage work.
- **Report after:** implementation/tests, benchmark freeze/replay, and final validation/PR.
- **Report to:** the owning coordinator through the linked child result. Do not message Chatterbox during automatic dispatch.

## Suggested Next Move

Run the `Completion Protocol` preflight before broad reads. Then read the
active milestone, card, `AGENTS.md`, architecture, contract, and guide from the
selected worktree. Start with the focused query-term and ranking tests, then
freeze and run the two benchmark cases.

## Completion Protocol

Use the standard worker completion protocol in the Northstar orchestrator
handoff template. Verify the tracked handoff at the exact pushed base; execute
only card `1114`; falsify every review-oracle counterexample; keep reserved
closeout surfaces for the coordinator; push the branch; and open a reviewable
PR without merging it.

The orchestrator will launch an independent reviewer in this same worker
workspace under a serial clean exact-head lease. The reviewer must use a
provider/model identity distinct from the authoring worker, post a durable
verdict naming the exact head SHA, and make no tracked changes.

- **Closeout refs:** card `1114`, roadmap `g09.007`, spec `121`, one dated evidence log under `docs/logs/2026-09/`, `CHANGELOG.md`, and named front doors

Before calling the runway complete, leave the card, roadmap, log, and next-task
state honest. The coordinator reconciles and closes those surfaces after merge.
