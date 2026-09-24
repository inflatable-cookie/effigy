---
kind: northstar-handoff
title: "Effigy Chatterbox continuation"
handoff_mode: chatterbox-continuation
chatterbox_mode: conversational-planning
dispatch_authority: chatterbox
status: ready-to-launch
base_required: pushed-main
---

## What This Thread Was Doing

This long-running Effigy Chatterbox took over the g09 closeout, refreshed the
project into g10, shaped several operator-raised product issues into bounded
canonical tasks, and dispatched approved work through Northstar Queue. The main
threads were named installed-skill execution with raw stdio passthrough,
catalog-scoped monorepo code graphs, published versus provisional task
surfaces, and a bounded structural/deep `doctor` split for very large repos.

The operator has asked for a fresh same-workspace Chatterbox using GPT-6 Sol at
medium reasoning. This is an ownership transfer, not a new planning lane or a
request to archive this source thread.

## Why It Matters

The implementation runway that occupied this conversation is now terminal, but
the semantic planning front doors have not caught up. The successor needs the
operator's settled preferences, the open triage state, and the exact Queue and
Paseo identities so it can repair drift before proposing another executable
frontier. Canonical docs remain authority; this handoff preserves only the live
conversation that those docs cannot recover.

## Current State

- Source Chatterbox: `539a5ed7-5c02-401f-aa17-9f318bd81217`.
- Workspace: `wks_8f1e5b27b72dd29b` at
  `/Users/tom/Dev/projects/effigy`.
- Integration state when written: clean `main == origin/main ==
  2a8df43e4582445620f49181fefb414d2ffafc9f`.
- Queue origin-transfer preflight plan:
  `3f7ee0a2be68cb3f1d66e06aa8a9be131d7fb09003dff75ccafdff350db272b5`.
- The preflight found no unfinished Queue tasks targeting the source
  Chatterbox; the exact transfer set is empty.
- Queue task `1b8c0629-0955-42d5-9883-739db1796f16` / g10.010 completed through
  PR #113. Merge commit: `1814c28207b194887fb8c99a92d6b95fedeb1673`;
  lifecycle closeout: `392a449b09b56eb80569277aa3a9c8fdd83d3942`.
- The lifecycle projections correctly show g10.010 complete and the g10 runway
  `planning_required`. Prose in `docs/roadmaps/README.md`,
  `docs/roadmaps/g10/README.md`, `docs/roadmaps/generation-index.md`,
  `docs/contracts/001-working-rules.md`, `docs/contracts/README.md`,
  `docs/specs/README.md`, and `docs/logs/README.md` still calls g10.010 ready
  or dispatchable. Treat that as current planning drift, not execution
  authority.
- No strict lane or ready worker handoff is active. No successor task is
  approved.
- Open triage is under `/Users/tom/Dev/projects/effigy/docs/triage/`:
  - `20260901-092640-feature-boundary-residual-open-design.md`;
  - `20260905-092527-release-gate-failure-diagnosability.md`;
  - `20260906-224721-release-gates-satisfied-by-hosted-evidence.md`;
  - `20260909-152106-consumer-adoption-cohort-expansion.md`;
  - `20260909-152107-vendored-effigy-skill-portfolio-sync.md`;
  - `20260917-212846-host-database-structural-check.md`;
  - `20260917-230600-per-worktree-container-identity-default.md`.
- `docs/triage/README.md` indexes only the first five notes. The two
  2026-09-17 notes are operator-directed Acowtancy intake and remain
  non-authoritative until reconciled and promoted.
- `PAPERCUTS.md` still contains some entries whose promoted tasks are already
  terminal. Reconcile evidence before closing or promoting anything; PAPERCUTS
  alone grants no execution authority.

## Boundaries

Remain Chatterbox: explore with the operator, reconcile triage, promote
confirmed canonical planning, and dispatch only after current execution
authorization. Do not implement product code by default, supervise workers,
review or merge PRs, infer a task from PAPERCUTS, or treat stale ready prose as
authority.

Preserve the shared-checkout rules in `AGENTS.md`. Never edit lifecycle
projection blocks manually. Never modify `.github/workflows/` or run release
mutations without explicit operator instruction. Keep release work, hosted-gate
delegation, S3/provider retirement, consumer portfolio sync, host-database
policy, and per-worktree container identity as separate decisions unless the
operator deliberately joins them.

Do not archive, stop, detach, rename, or otherwise dispose of this source agent
or workspace during the refresh. Queue attention transfer must preserve task,
worker, reviewer, coordinator, PR, workspace, dependency, and original-origin
identity. The transfer set is empty, but still verify the exact plan.

## Important Context

- The operator wants Northstar Queue as Effigy's implementation dispatch path.
  Chatterbox owns product meaning and canonical promotion; Queue owns durable
  orchestration, review, merge, and hook closeout.
- The operator prefers a small explicit public task library. `[tasks]` is the
  maintained published surface; lifecycle-labelled `[drafts]` holds temporary
  proofs and environments. Expiry is cleanup evidence, not automatic deletion.
- Monorepo graph segmentation comes only from declared catalogs. Each catalog
  declares its own graph posture; `independent` means its own database. Root
  work prunes segmented members and whole-workspace fan-out must be explicit.
- Default `effigy doctor` is now structural and bounded; `doctor --deep` owns
  content scans and selected-scope `health`, with shared inventory, exact
  incremental cache facts, and visible deadline failure. The implementation is
  complete; do not reopen it from stale task prose.
- Named skill resolution and `effigy skill run --stdio passthrough` shipped in
  g10.001. Passthrough is deliberately distinct from Effigy's `--json`
  envelope and adds no transport bytes.
- The two newest triage notes came from real Acowtancy damage and concurrency
  failures. One asks for a cheap structural doctor finding for host PostgreSQL
  tooling. The other asks for safe per-worktree container identity and teardown
  by default. Their urgency is evidence, not automatic readiness; inspect
  current architecture and ask the operator which deserves the next runway.
- Current `main` also includes `2a8df43e4`, a direct fix for resolving
  `Cargo.lock` Git-show paths from the Git toplevel. It is not a Northstar task
  or an implied next lane.
- The requested successor runtime is exactly `codex/gpt-6-sol`, full-access,
  medium reasoning. Provider inspection confirmed it is available. The
  configured exact-model profile is worker-oriented, so launch uses the exact
  provider/model/settings directly rather than silently substituting a profile
  or model.

## Suggested Next Move

After ownership transfer, reload `main` and start with a short Northstar state
reconciliation: confirm g10.010's terminal projection and Queue record, index
the two live triage notes as needed, and make the active front doors stop
advertising completed work. Then ask Tom which planning conversation should
lead the next runway: per-worktree container isolation, host-database structural
diagnosis, one of the older retained candidates, or a fresh problem. Do not
manufacture g10.011 merely to fill an empty frontier.

## Completion Protocol

Remain read-only until this source sends the exact follow-up `Ownership
transfer complete`. Then enter normal Northstar Chatterbox mode from this
handoff and current canonical docs. You become the sole active Effigy planning,
promotion, Queue-ruling, and coordinator-direction authority in workspace
`wks_8f1e5b27b72dd29b`; this source remains visible as history and must not
compete.

The Queue transfer applies only to preflight plan
`3f7ee0a2be68cb3f1d66e06aa8a9be131d7fb09003dff75ccafdff350db272b5` and
its empty exact task set. Before any future dispatch, re-read the current
architecture, contracts, ready task, handoff, and operator approval. If
front-door reconciliation changes canonical planning, validate it, review the
semantic diff, commit and push on `main`, and report the exact commit. No next
implementation task is currently authorized.
