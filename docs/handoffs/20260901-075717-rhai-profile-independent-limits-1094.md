---
title: Rhai profile-independent limits 1094 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-075717-rhai-profile-independent-limits-1094.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

The orchestrator reconciled the Northstar papercut sweep against current
Effigy `main`. The graph-timeout report remains a planning question; the Rhai
debug/release parser-limit defect is the next bounded ready lane.

This dispatches one implementation lane. No transcript or second prompt is
part of the authority chain.

## Why It Matters

A checked-in script must not pass the installed release binary and fail the
documented source-build fallback solely because the dependency selects a lower
parser limit under `debug_assertions`.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `fc984a61d7b24cb2999d54532f1114e6425cef6e`
- **Pushed main verification:** planning commit containing this handoff must be
  pushed and local `HEAD == origin/main` before launch
- **Planning checkout:** clean before this planning batch
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight
- **Planning artifacts included at the base:** roadmap `g08.039`, spec `112`,
  card `1094`, planning log, queue normalization, and this handoff
- **Worker branch:** `worker/g08-039-rhai-profile-limits-1094`
- **Worker worktree:** launcher-managed; named fallback
  `/Users/tom/Dev/worktrees/effigy-rhai-profile-limits-1094`
- **Worktree creation command:** only if preflight permits:
  `git worktree add /Users/tom/Dev/worktrees/effigy-rhai-profile-limits-1094 -b worker/g08-039-rhai-profile-limits-1094 origin/main`
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree
  first, named/manual fallback only when required
- **Required sibling worktree links:** none
- **Active spec lane:** [`112`](../specs/archive/112-rhai-profile-independent-limits-strict-lane.md)
  (archived at closeout)
- **Roadmap milestone:** [`g08.039`](../roadmaps/g08/039-rhai-profile-independent-limits-papercut.md)
- **Ready cards, in order:** [`1094`](../roadmaps/g08/batch-cards/1094-fix-rhai-profile-dependent-expression-limits.md)
  (complete at closeout)- **Allowed runway:** card `1094` only
- **Remaining card budget:** one
- **Dispatch topology:** serial Effigy papercut lane
- **Parallel safety check:** no open Effigy PR and no active implementation
  worker owns `effigy-rhai` engine construction
- **Canonical refs:** `AGENTS.md`; contract `001`; guide `061`;
  `PAPERCUTS.md`
- **Review oracle:** card `1094`, five counterexamples
- **Model capability profile:** capable coding worker, medium reasoning
- **Tool/runtime restrictions:** no `.github/workflows/`, release mutation,
  S3/provider extraction, public limit configuration, or unrelated papercut
- **Required validation:** focused debug and release `effigy-rhai` tests,
  `effigy perf:docs-context-benchmark`, `effigy qa`, fmt, clippy, diff check
- **PR base/head:** current pushed `main` /
  `worker/g08-039-rhai-profile-limits-1094`
- **PR URL:** https://github.com/inflatable-cookie/effigy/pull/67
- **Review state:** awaiting orchestrator review after worker completion
- **Merge path:** orchestrator after accepted review of the current head and
  passing required checks
- **Exact head:** `ce037e698012b0196426e0bda2ad094b262b82b7`
## Boundaries

- **In scope:** reproduce, diagnose, and fix card `1094`; exact `64` / `32`
  expression limits; recurrence tests; first-party script proof; docs,
  changelog, papercut, evidence, and lane closeout
- **Out of scope:** configurable or unlimited limits; call-stack, operation,
  data, module, host API, S3, provider, graph, catalog-pack, consumer, release,
  or CLI changes
- **Outcome shape:** complete fix, not diagnostics-only
- Do not invent architecture, change contracts, widen the roadmap, or choose an
  unresolved product/API/persistence/security decision.
- Work only in the clean worker worktree selected by `Completion Protocol`.
- Do not merge the PR. Merge belongs to the orchestrator.

## Important Context

- **Planning lineage:** bounded papercut interruption after completed card
  `1093`; catalog-pack acquisition remains the next planning checkpoint
- **Why ready:** observed failure, exact non-breaking limits, owner/seam,
  acceptance, adversarial oracle, validation, and stop conditions are settled
- **Decisions:** preserve release defaults `64` / `32`; change expression depth
  only; keep finite guards
- **Open tensions:** exact Rhai parser nesting syntax may require a generated
  test fixture, but must not change the approved thresholds
- **Report after:** configured engine and focused adversarial proof form one
  coherent chunk; report again at pushed PR closeout
- **Report to:** the orchestrator through Paseo notification

## Suggested Next Move

Run the `Completion Protocol` preflight before broad reads. Then read
`AGENTS.md`, spec `112`, roadmap `g08.039`, card `1094`, and guide `061`.

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

- Own reproduction through the smallest complete fix, cleanup, validation,
  evidence, and PR.
- Keep commits aligned with meaningful chunks.
- Stop on scope expansion, a new public threshold, a non-expression limit
  change, missing authority, or validation that changes the plan.
- Report meaningful chunks through the active control plane.

### When the assigned runway is complete

1. Run the validation named above.
2. Falsify all five card-oracle rows and map each to proof.
3. Close card, roadmap, spec, selected papercut, evidence log, and front-door
   state. Return `Next Task` to catalog-pack acquisition planning.
4. Push the worker branch and open a PR against current pushed `main`.
5. Link the spec, milestone, card, evidence, validation, and unresolved items.
6. Report the PR URL and exact head. Do not merge.

### Review and merge path

The orchestrator reviews the exact PR head and records its verdict on GitHub.
Requested changes return to this same worker. Accepted exact head plus passing
checks and mergeability authorizes the orchestrator merge without another
prompt.

- **Closeout refs:** card `1094`; roadmap `g08.039`; spec `112`; dated evidence
  log; `PAPERCUTS.md`; active front doors

### Handoff closeout

Leave the runway honest. If blocked, record the blocker and stop instead of
marking the lane complete.
