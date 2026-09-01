---
title: Rhai storage create-only worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-165314-rhai-storage-create-only-worker.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, rhai, storage, concurrency]
---

## What This Thread Was Doing

Bovine PR 32 proved that Effigy's Rhai storage surface cannot atomically create
an absent object. This dispatch owns the smallest additive Effigy repair.

## Why It Matters

HEAD then PUT cannot prevent two writers from both seeing absence and one
overwriting the other. Bovine cannot honestly close its mutation boundary
without a condition carried by the write itself.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `1c0ebe9c8e929d8fcf87a02da2102d2059e27e18`
- **Pushed main verification:** local `HEAD` equalled `origin/main` before this
  handoff batch
- **Planning checkout:** clean before this handoff batch
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight
- **Planning artifacts included at the base:** contract `044`, spec `114`,
  roadmap `g08.044`, card `1099`, and this handoff once pushed
- **Worker branch:** `worker/rhai-storage-create-only`
- **Worker worktree:** Paseo-managed generated worktree
- **Worktree creation command:** Paseo `branch-off` from `origin/main`
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree
  first, manual fallback only when required
- **Required sibling worktree links:** none
- **Active spec lane:** `docs/specs/114-rhai-storage-create-only-strict-lane.md`
- **Roadmap milestone:** `docs/roadmaps/g08/044-rhai-storage-create-only.md`
- **Ready cards, in order:** card `1099`
- **Allowed runway:** card `1099` only
- **Remaining card budget:** one card and one PR
- **Dispatch topology:** one dependency lane; Bovine PR 32 stays paused
- **Parallel safety check:** separate repository and mutable surface from Bovine
  PRs 30–32
- **Surfaces this lane owns:** `crates/effigy-rhai` storage host/tests/surface,
  focused storage/Rhai docs, changelog, card/roadmap/spec closeout, one log
- **Integration ownership:** this worker owns Effigy closeout; the Bovine
  orchestrator owns downstream resumption
- **Merge ordering:** same-repository PRs merge one at a time
- **Canonical refs:** architecture `026`; contracts `001`, `043`, and `044`
- **Review oracle:** card `1099` `## Review Oracle`
- **Model capability profile:** capable ordinary Rust implementation worker;
  orchestrator retains frontier exact-head review
- **Frontier-worker justification:** none
- **Tool/runtime restrictions:** no live object storage, credentials, release,
  workflow, Bovine edits, provider framework, or arbitrary condition support
- **Required validation:** focused storage tests, changed-impact checks,
  `effigy test --plan`, `effigy qa`, `effigy doctor`, `git diff --check`
- **PR base/head:** `main` <- `worker/rhai-storage-create-only`
- **PR URL:** pending
- **Review state:** awaiting implementation PR
- **Merge path:** orchestrator after accepted exact-head review and passing gates

## Boundaries

- **In scope:** card `1099` from reproduction through additive repair, proof,
  docs, changelog, evidence, push, and PR
- **Out of scope:** live R2/S3, Bovine edits, storage removal/extraction, new
  provider architecture, general conditional/versioned writes, retries,
  release, workflows, and catalog-pack publication
- **Outcome shape:** smallest complete contract-valid fix, not diagnostics only
- Stop on any card stop condition. Do not weaken the atomicity claim.
- Work only in the selected clean worker worktree. Do not merge.

## Important Context

- The vendored client appears to support `if_none_match("*")`; prove that
  against the actual current API before editing.
- The public decision is settled: optional boolean `create_only`; omitted or
  false preserves current behavior.
- Provider errors may carry signed URLs or hostile bodies. The stable Rhai
  collision diagnostic must not include them.
- Report after the focused fixture proves one winner and unchanged loser state.

## Suggested Next Move

Run the worker preflight. Read `AGENTS.md`, spec `114`, roadmap `g08.044`, card
`1099`, architecture `026`, and contracts `001`, `043`, and `044`. Reproduce
the race in the local fixture, then implement the bounded option.

## Completion Protocol

Before broad reads, run `git rev-parse --show-toplevel`,
`git branch --show-current`, `git status --porcelain`, and
`git worktree list --porcelain`. Accept a clean registered non-`main` launcher
worktree. Otherwise follow `.agents.local.env`; never guess a fallback path or
discard dirty state.

Fetch with `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git
fetch origin`. Confirm `HEAD == origin/main`, the planning base is an ancestor,
and the tracked handoff matches the absolute file. Required sibling links are
none.

Execute only card `1099`. Use Northstar strict everyday Rust authoring and the
repository's Effigy routes. Stop on missing atomic support, redaction problems,
scope expansion, or validation that changes the plan.

When complete, run all required gates, falsify every oracle row, close the
card/roadmap/spec honestly, add the evidence log and changelog entry, commit,
push, open one PR to `main`, and return its URL and exact head. Do not merge.

The orchestrator reviews the exact head. Requested changes return to this same
worker. Accepted review plus passing checks authorizes the orchestrator merge;
the downstream Bovine lane remains separately owned.
