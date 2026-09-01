---
title: Child-catalog suite registry papercut worker
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-175827-child-catalog-suite-registry-1100.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts, test, containers]
---

## Objective

Execute card `1100`: preserve the originating repository's ancestor
`[containers]` registry while expanding a suite task reference at a child
catalog cwd.

## Launch State

- Repository: `/Users/tom/Dev/projects/effigy`
- Planning base: `b1295f8f1`
- Worker branch: `worker/g08-045-child-catalog-suite-registry-1100`
- Roadmap: `docs/roadmaps/g08/045-child-catalog-suite-registry-papercut.md`
- Card and review oracle:
  `docs/roadmaps/g08/batch-cards/1100-preserve-ancestor-container-registry.md`
- Contracts: `001`, `038`
- Required sibling links: none
- Allowed runway: card `1100` only; one PR
- Worker class: day-to-day; bounded ordinary Rust resolver repair
- Frontier implementation justification: none

## Ownership And Parallel Safety

Own only `crates/effigy-builtin/src/test/**`, focused test-orchestration
fixtures/tests, and `crates/effigy-managed/src/run_spec/**` if the smallest fix
requires it. Add one unique card closeout log and update only this card/roadmap.

Do not edit shared front doors, `PAPERCUTS.md`, `CHANGELOG.md`, contracts,
guide `079`, Acowtancy, or sibling lane files. Card `1101` owns root runner
graph/docs timeout code. Card `1102` owns codegraph docs-context ranking. The
catalog publication delegate owns its triage packet. Card `1099` is separate.
The orchestrator owns integration, exact-head review, and serial merge order.

## Boundaries

Use a synthetic parent/child recurrence. Preserve child cwd, child explicit
override precedence, and normal direct-child discovery. Acowtancy's workaround
must remain until its owner revalidates. Stop on Acowtancy edits, manifest
grammar, ambient ancestor discovery, broad runner redesign, or shared-surface
overlap.

## Validation And Evidence

Falsify every card oracle row. Run focused planning/execution tests,
`effigy graph affected` plus direct targets, `effigy test --plan`, `effigy qa`,
fmt, clippy `-D warnings`, and `git diff --check`. Add one dated evidence log
mapping each oracle row to exact proof.

## Completion Protocol

Run the worker preflight before broad reads: verify a clean registered non-main
worktree, fetch with bounded noninteractive SSH, confirm `HEAD == origin/main`,
confirm `b1295f8f1` is an ancestor, and confirm this tracked absolute handoff.
Read `AGENTS.md`, the roadmap, card, and contracts. Implement only card `1100`.
Commit, push, open one PR to `main`, and report the URL and exact head. Do not
merge. Review revisions return to this same worker.
