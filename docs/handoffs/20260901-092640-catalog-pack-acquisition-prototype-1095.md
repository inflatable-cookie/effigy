---
title: Catalog-pack acquisition prototype 1095 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-092640-catalog-pack-acquisition-prototype-1095.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, catalog-pack]
---

## What This Thread Was Doing

The orchestrator resolved the catalog-pack acquisition policy under architecture
`026` and contract `043`, promoted it into strict spec `113`, and made one
in-repository prototype card ready.

This dispatches one implementation lane. No transcript or second prompt is
part of the authority chain.

## Why It Matters

Effigy must separate ownership of concrete catalog definitions without making
service, workspace, container, task, source-install, or offline use harder. The
prototype proves the acquisition and recovery machinery before any asset or
release-distribution cutover.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `cff0e9d2d42a1498acdaca4a8a11498740365cb2`
- **Pushed main verification:** planning commit containing this handoff must be
  pushed and local `HEAD == origin/main` before launch
- **Planning checkout:** clean before this planning batch
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight
- **Planning artifacts at launch:** architecture `026`, contract `043`, roadmap
  `g08.040`, spec `113`, card `1095`, planning log, residual triage note, and
  this handoff
- **Worker branch:** `worker/g08-040-catalog-pack-acquisition-1095`
- **Worker worktree:** launcher-managed; named fallback
  `/Users/tom/Dev/worktrees/effigy-catalog-pack-acquisition-1095`
- **Worktree creation command:** only if preflight permits:
  `git worktree add /Users/tom/Dev/worktrees/effigy-catalog-pack-acquisition-1095 -b worker/g08-040-catalog-pack-acquisition-1095 origin/main`
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree
  first, named/manual fallback only when required
- **Required sibling worktree links:** none
- **Active spec lane:** [`113`](../specs/113-catalog-pack-acquisition-prototype-strict-lane.md)
- **Roadmap milestone:** [`g08.040`](../roadmaps/g08/040-catalog-pack-acquisition-prototype.md)
- **Ready cards, in order:** [`1095`](../roadmaps/g08/batch-cards/1095-prototype-catalog-pack-acquisition.md)
- **Allowed runway:** card `1095` only
- **Remaining card budget:** one
- **Dispatch topology:** serial catalog-pack prototype
- **Parallel safety check:** no open Effigy PR and no separate active Effigy
  worker owns catalog resolution, service routing, artifact acquisition, or
  doctor pack health
- **Canonical refs:** `AGENTS.md`; contracts `001` and `043`; architecture
  `026`; catalog guides `063`, `067`, and `071`; artifact guide `072`
- **Review oracle:** card `1095`, eight counterexamples
- **Model capability profile:** frontier coding worker, high reasoning because
  this crosses persistence, public CLI/JSON, compatibility, and OCI boundaries
- **Tool/runtime restrictions:** no `.github/workflows/`, release mutation,
  official publication, live default OCI coordinate, concrete catalog move,
  general extension transport, S3, or Rhai-provider work
- **Required validation:** focused catalog/artifact/CLI/runner/doctor/assembly
  tests, `effigy qa`, fmt, clippy, and diff check
- **PR base/head:** current pushed `main` /
  `worker/g08-040-catalog-pack-acquisition-1095`
- **Review state:** orchestrator review required on the exact pushed head
- **Merge path:** orchestrator after accepted exact-head review, passing required
  checks, and confirmed mergeability

## Boundaries

- **In scope:** implement card `1095` completely, including typed pack model,
  versioned state, atomic activation, explicit OCI/local install, selection,
  visible fallback, status/rollback/reset, doctor, docs, evidence, and closeout
- **Out of scope:** public no-argument update, official artifact/repository,
  catalog asset extraction, workflow/install/release edits, implicit network,
  signing policy expansion, marketplace/general extension work, S3, and Rhai
  provider movement
- **Outcome shape:** complete usable in-repository prototype, not a design-only
  or diagnostics-only PR
- Do not invent architecture, choose an official registry coordinate, or widen
  the roadmap.
- Work only in the clean worker worktree selected by `Completion Protocol`.
- Do not merge the PR. Merge belongs to the orchestrator.

## Important Context

- **Planning lineage:** feature-boundary audit -> help-first discovery ->
  operator-confirmed catalog acquisition policy -> card `1095`
- **Why ready:** common-path behavior, source/trust boundary, persistence and
  failure semantics, public prototype grammar, acceptance, oracle, validation,
  and stop conditions are settled
- **Core invariant:** project override > user override > active installed pack >
  compiled baseline; normal commands never query OCI
- **Failure invariant:** validate before atomic activation; candidate failure
  preserves prior active state; later unhealthy state falls back visibly
- **Publication boundary:** fixed-channel update is modeled and adapter-tested,
  but no public `update` command exists until the official artifact exists
- **Open tensions:** exact internal module/store layout is worker judgment so
  long as one selection implementation and one transport seam remain; return a
  new public compatibility, authenticity, or retention decision to planning
- **Report after:** domain/store transaction and selection/CLI integration are
  meaningful chunks; report again at pushed PR closeout
- **Report to:** the orchestrator through Paseo notification

## Suggested Next Move

Run the `Completion Protocol` preflight before broad reads. Then read
`AGENTS.md`, architecture `026`, contracts `001` and `043`, spec `113`, roadmap
`g08.040`, card `1095`, and the relevant catalog/artifact guides. Use
`effigy graph` for ownership/impact questions after the worktree decision.

## Completion Protocol

### Before you start

1. This handoff's worker metadata activates worker mode. Before broad reads,
   run `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. Accept a clean, registered, non-`main` launcher worktree. Record its actual
   path/branch and do not create another because generated names differ.
3. Only if unusable, inspect the named fallback, then `.agents.local.env` and
   `AGENTS_WORKTREE_CONTAINER_DIR`. Never use `/tmp`; never clean or discard a
   dirty checkout. Report a launcher-supplied dirty or `main` worktree.
4. From the selected worktree run
   `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm `HEAD == origin/main`, confirm the planning base above is an ancestor,
   and load this handoff from tracked `HEAD`. Stop if the absolute file differs.
5. Required sibling links: none.
6. Read the active milestone, card, spec, `AGENTS.md`, and canonical refs.
7. Run the repo's cheap orientation checks and record what ran.

### While you work

- Own the complete prototype through implementation, cleanup, validation,
  evidence, closeout, and PR.
- Keep commits aligned with meaningful chunks.
- Falsify transaction, compatibility, selection, and network-negative claims
  before closeout.
- Stop on scope expansion, a new public trust/compatibility/retention choice,
  release/workflow need, live publication dependency, or validation that changes
  the plan.
- Report meaningful chunks through the active control plane.

### When the assigned runway is complete

1. Run the validation named above.
2. Falsify all eight card-oracle rows and map each to exact proof.
3. Close card, roadmap, spec, evidence log, and every front-door currentness
   surface. Preserve the residual feature-boundary triage note.
4. Return `Next Task` to planning for official pack publication and
   concrete-asset cutover under contract `043`; do not mark that lane ready.
5. Push the worker branch and open a PR against current pushed `main`.
6. Link the spec, milestone, card, evidence, validation, and unresolved items.
7. Report the PR URL and exact head. Do not merge.

### Review and merge path

The orchestrator reviews the exact PR head and records its verdict on GitHub.
Requested changes return to this same worker. Accepted exact head plus passing
checks and mergeability authorizes the orchestrator merge without another
prompt.

- **Closeout refs:** card `1095`; roadmap `g08.040`; spec `113`; dated evidence
  log; active front doors; residual feature-boundary triage note

### Handoff closeout

Leave the runway honest. If blocked, record the blocker and stop instead of
marking the lane complete.
