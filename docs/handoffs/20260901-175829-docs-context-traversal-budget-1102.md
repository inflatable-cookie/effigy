---
title: Docs-context traversal budget papercut worker
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-175829-docs-context-traversal-budget-1102.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts, docs, graph, traversal]
---

## Objective

Execute card `1102`: reserve bounded result capacity so typed-relation evidence
remains reachable when 0-hop lexical candidates saturate `max-sections`.

## Launch State

- Repository: `/Users/tom/Dev/projects/effigy`
- Planning base: `b1295f8f1`
- Worker branch: `worker/g08-047-docs-context-traversal-budget-1102`
- Roadmap: `docs/roadmaps/g08/047-docs-context-traversal-budget-papercut.md`
- Card and review oracle:
  `docs/roadmaps/g08/batch-cards/1102-reserve-docs-context-traversal-slot.md`
- Contracts: `001`, `041`
- Required sibling links: none
- Allowed runway: card `1102` only; one PR
- Worker class: day-to-day; bounded deterministic selection repair
- Frontier implementation justification: none

## Ownership And Parallel Safety

Own `crates/effigy-codegraph/src/docs_context/**`, focused codegraph tests, and
focused CLI tests only if public output requires them. The existing benchmark
script/test may change only if a deterministic traversal proof cannot live in
the focused fixture without weakening the card. Add one unique card closeout
log and update only this card/roadmap.

Do not edit shared front doors, `PAPERCUTS.md`, `CHANGELOG.md`, contracts,
guide `079`, root runner graph/docs timeout files, or sibling lane files. Card
`1100` owns test task-ref resolution. Card `1101` owns timeout/progress. The
publication delegate and card `1099` stay separate. The orchestrator owns
integration, exact-head review, and serial merge order.

## Boundaries

Keep one ranking/selection implementation. Preserve the best direct result,
one-slot semantics, byte integrity, relevance gates, provenance, and hop
truncation. Stop on a new public query mode, inference, second ranker, JSON
schema break, refresh change, unrelated benchmark rewrite, or shared-surface
overlap.

## Validation And Evidence

Falsify every card oracle row. Run focused codegraph docs-context tests, focused
CLI tests if needed, `effigy perf:docs-context-benchmark`, `effigy graph
affected` plus direct targets, `effigy qa`, fmt, clippy `-D warnings`, and
`git diff --check`. Add one dated evidence log mapping each row to exact proof.

## Completion Protocol

Run the worker preflight before broad reads: verify a clean registered non-main
worktree, fetch with bounded noninteractive SSH, confirm `HEAD == origin/main`,
confirm `b1295f8f1` is an ancestor, and confirm this tracked absolute handoff.
Read `AGENTS.md`, the roadmap, card, and contracts. Implement only card `1102`.
Commit, push, open one PR to `main`, and report the URL and exact head. Do not
merge. Review revisions return to this same worker.
