---
title: Docs-context cold refresh papercut worker
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-175828-docs-context-time-budget-1101.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts, docs, graph, timeout]
---

## Objective

Execute card `1101`: give cold/stale `effigy docs context` refresh the shared
graph wall-clock bound, typed timeout evidence, and stderr progress notice.

## Launch State

- Repository: `/Users/tom/Dev/projects/effigy`
- Planning base: `b1295f8f1`
- Worker branch: `worker/g08-046-docs-context-time-budget-1101`
- Roadmap: `docs/roadmaps/g08/046-docs-context-time-budget-papercut.md`
- Card and review oracle:
  `docs/roadmaps/g08/batch-cards/1101-bound-docs-context-cold-refresh.md`
- Contracts: `001`, `041`
- Required sibling links: none
- Allowed runway: card `1101` only; one PR
- Worker class: day-to-day; bounded command-boundary Rust repair
- Frontier implementation justification: none

## Ownership And Parallel Safety

Own `src/runner/graph_command.rs`, `src/runner/docs_command/context.rs`, the
smallest shared time-budget helper if extracted, and focused runner tests. Add
one unique card closeout log and update only this card/roadmap.

Do not edit shared front doors, `PAPERCUTS.md`, `CHANGELOG.md`, contracts,
guide `079`, codegraph docs-context rank/selection files, or sibling lane files.
Card `1100` owns test task-ref resolution. Card `1102` owns codegraph result
selection. The publication delegate and card `1099` stay separate. The
orchestrator owns integration, exact-head review, and serial merge order.

## Boundaries

Reuse the existing `EFFIGY_GRAPH_TIMEOUT_MS` parser, bounded operation, schema,
health snapshot, and recovery. Progress is stderr-only and only when refresh is
cold/stale. Preserve `0` as disabled. Stop on a daemon, second index/refresh or
timeout model, new public flag/schema, new cancellation guarantee, ranking
change, or shared-surface overlap.

## Validation And Evidence

Falsify every card oracle row. Run focused graph/docs timeout, stderr, text,
JSON, warm, disabled-bound, and graph-regression tests; `effigy graph affected`
plus direct targets; `effigy qa`; fmt; clippy `-D warnings`; and
`git diff --check`. Add one dated evidence log mapping each row to exact proof
and record the measured bounded result.

## Completion Protocol

Run the worker preflight before broad reads: verify a clean registered non-main
worktree, fetch with bounded noninteractive SSH, confirm `HEAD == origin/main`,
confirm `b1295f8f1` is an ancestor, and confirm this tracked absolute handoff.
Read `AGENTS.md`, the roadmap, card, and contracts. Implement only card `1101`.
Commit, push, open one PR to `main`, and report the URL and exact head. Do not
merge. Review revisions return to this same worker.
