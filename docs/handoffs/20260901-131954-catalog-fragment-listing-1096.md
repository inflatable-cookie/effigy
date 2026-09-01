---
title: Catalog fragment listing 1096 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-131954-catalog-fragment-listing-1096.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

PR 69 closed the environment-lock papercut. Card `1096` is the next bounded
ready Effigy papercut: `service list` advertises root catalog assets as service
fragments.

This dispatches one implementation lane. No transcript or second prompt is
part of the authority chain.

## Why It Matters

The operator inventory must distinguish callable catalog fragments from
documentation and example files. The filesystem layers already use
`service.toml` as that boundary; the bundled inventory must agree.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `54d67af87ce9b8dd1c1fd166d6b7c1c752e870a3`
- **Pushed-main requirement:** the commit containing this handoff must be
  pushed and local `HEAD == origin/main` before launch
- **Worker branch:** `worker/g08-041-catalog-fragment-listing-1096`
- **Worker worktree:** launcher-managed
- **Required sibling worktree links:** none
- **Roadmap:** [`g08.041`](../roadmaps/g08/041-catalog-fragment-listing-papercut.md)
- **Ready card:** [`1096`](../roadmaps/g08/batch-cards/1096-fix-catalog-fragment-listing.md)
- **Allowed runway:** card `1096` only
- **Dispatch topology:** one ordinary bounded implementation lane
- **Parallel safety:** no open Effigy PR or worker owns `effigy-catalog`
  fragment inventory or this lane's closeout surfaces
- **Worker class:** day-to-day
- **Worker-profile reason:** small, settled Rust inventory correction with an
  explicit oracle; no exceptional reasoning or material consequence warrants
  a frontier implementation worker
- **Frontier-worker justification:** none
- **Required validation:** card `1096`
- **PR base/head:** current pushed `main` /
  `worker/g08-041-catalog-fragment-listing-1096`
- **Merge path:** orchestrator after accepted exact-head review, passing checks,
  and clean mergeability

## Boundaries

- **In scope:** implement and close card `1096`, including focused tests,
  papercut/evidence/planning closeout, and changelog only if user-visible policy
  requires it.
- **Out of scope:** pack acquisition/publication/update/retention, concrete
  assets, catalog schema/layering, CLI grammar/schema, release/workflow, S3,
  providers, and unrelated papercuts.
- Work only in the launcher worktree. Preserve unrelated work.
- Do not merge the PR. Merge belongs to the orchestrator.

## Important Context

- **Why ready:** observed output, exact membership rule, owner, acceptance,
  adversarial oracle, validation, and stop conditions are settled.
- **Planning posture:** bounded papercut interruption; official publication
  remains paused and returns as the Next Task after closeout.
- **Open tensions:** none inside this card. A need to change fragment schema or
  catalog layers is a stop condition.
- **Report after:** coherent implementation/test batch and pushed PR closeout.

## Suggested Next Move

Run worker preflight, then read `AGENTS.md`, contract `001`, roadmap `g08.041`,
card `1096`, the selected papercut, and `crates/effigy-catalog/src/fragment.rs`.

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

1. Run card validation and map all five oracle rows to exact proof.
2. Close card, roadmap, selected papercut, evidence log, and active Next Task
   pointers. Return them to publication planning; do not open that lane.
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
