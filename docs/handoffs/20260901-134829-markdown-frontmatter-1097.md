---
title: Markdown frontmatter extraction 1097 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-134829-markdown-frontmatter-1097.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

PR 70 closed card `1096`. Card `1097` is the next bounded ready Effigy
papercut: leading YAML frontmatter can become one synthetic setext heading in
the documentation graph.

This dispatches one implementation lane. No transcript or second prompt is
part of the authority chain.

## Why It Matters

The synthetic heading consumes bounded agent-context budget and carries a
large, useless display name. The repair must preserve the metadata facts,
relations, and exact provenance that make repository-defined docs profiles
useful.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `c4cea6acba39b653cc97da9b72c6e40428c66530`
- **Pushed-main requirement:** the commit containing this handoff must be
  pushed and local `HEAD == origin/main` before launch
- **Worker branch:** `worker/g08-042-markdown-frontmatter-1097`
- **Worker worktree:** launcher-managed
- **Required sibling worktree links:** none
- **Roadmap:** [`g08.042`](../roadmaps/g08/042-markdown-frontmatter-extraction-papercut.md)
- **Ready card:** [`1097`](../roadmaps/g08/batch-cards/1097-fix-markdown-frontmatter-heading-extraction.md)
- **Allowed runway:** card `1097` only
- **Dispatch topology:** one ordinary bounded implementation lane
- **Parallel safety:** no open Effigy PR or worker owns Markdown extraction or
  this lane's closeout surfaces
- **Named serial edge:** merge this same-repository lane before another worker
  takes shared papercut/front-door closeout surfaces
- **Worker class:** day-to-day
- **Worker-profile reason:** bounded parser correction with settled metadata and
  provenance invariants; it needs careful ordinary Rust judgment, not frontier
  implementation reasoning
- **Frontier-worker justification:** none
- **Required validation:** card `1097`
- **PR base/head:** current pushed `main` /
  `worker/g08-042-markdown-frontmatter-1097`
- **Merge path:** orchestrator after accepted exact-head review, passing checks,
  and clean mergeability

## Boundaries

- **In scope:** implement and close card `1097`, including focused tests,
  contract/papercut/evidence/planning closeout, and user-facing docs/changelog
  where required.
- **Out of scope:** ranking/budgets/traversal, graph storage, docs-profile
  grammar, graph refresh/timeout, parser dependency upgrades, CLI/JSON changes,
  Northstar-specific runtime rules, catalog-pack publication, release/workflow,
  S3, and unrelated papercuts.
- Work only in the launcher worktree. Preserve unrelated work.
- Do not merge the PR. Merge belongs to the orchestrator.

## Important Context

- **Why ready:** observed failure, generic delimiter boundary, metadata and
  provenance invariants, owner, acceptance, adversarial oracle, validation, and
  stop conditions are settled.
- **Planning posture:** bounded papercut interruption; official publication
  remains paused and returns as the Next Task after closeout.
- **Open tensions:** none inside the card. A need to change profile semantics or
  parser dependencies is a stop condition.
- **Report after:** coherent implementation/test batch and pushed PR closeout.

## Suggested Next Move

Run worker preflight, then read `AGENTS.md`, contracts `001` and `041`, roadmap
`g08.042`, card `1097`, the selected papercut, and
`crates/effigy-codegraph/src/language/markdown/extract.rs`.

## Completion Protocol

### Before you start

1. Before broad reads, run `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`.
2. Reuse a clean registered non-`main` launcher worktree regardless of its
   generated path or branch spelling. Do not create another.
3. Run
   `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm `HEAD == origin/main`, the planning base is an ancestor, and this
   handoff is tracked at `HEAD`; stop if the absolute file differs.
4. Required sibling links: none.
5. Read the named authority surfaces and run cheap orientation checks.

### While you work

- Own reproduction through the smallest complete fix, tests, evidence, and
  closeout.
- Implement one coherent batch before broad validation.
- Stop on any card stop condition or planning expansion.
- Report meaningful chunks through Paseo.

### When the runway is complete

1. Run card validation and map all six oracle rows to exact proof.
2. Close card, roadmap, selected papercut, contract Next Task, evidence log,
   and active Next Task pointers. Return them to publication planning; do not
   open that lane.
3. Commit, push the worker branch, and open one PR against current pushed
   `main`.
4. Report PR URL, exact head, validation, unresolved items, and docs QA
   classification. Do not merge.

### Review and merge path

The orchestrator reviews the exact PR head and records its verdict on GitHub.
Requested changes return to this worker. Accepted exact head plus passing
checks and mergeability authorizes orchestrator merge without another prompt.

### Handoff closeout

Leave the runway honest. If blocked, record the blocker and stop rather than
marking the card complete.
