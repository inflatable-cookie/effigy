---
kind: northstar-handoff
title: "Effigy g10.014 — Resolve compatible Dependabot lock updates"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
base_required: pushed-main
queue_dispatch: northstar-queue
queue_approval: "Operator asked to resolve all open Dependabot PRs and confirmed consolidated reviewed batch PRs on 2026-09-24."
queue:
  capability: general
---

## What This Thread Was Doing

The operator wants the ten open Dependabot PRs resolved in consolidated, reviewed current-base batches. Implement the first ready batch, [g10.014](../roadmaps/g10/014-resolve-compatible-dependabot-lock-updates.md), through the Queue worker PR loop. The task card owns scope and acceptance.

## Why It Matters

Six compatible dependency updates remain absent from the 0.13.0-era lockfile. Their bot heads were built against different older bases and overlap on `Cargo.lock`. One aggregate replacement gives a coherent tested resolution and clear bot-PR disposition.

## Current State

- Start from pushed integration `main` after the published `v0.13.0` tag and documentation closeout. Queue creates the isolated worker workspace.
- Target bot PRs: #79, #80, #96, #97, #99, #100. Their old green checks are evidence for those heads only, not for this aggregate.
- `g10.015` is approved downstream and must not write `Cargo.lock` until this task reaches terminal closeout.
- The Chatterbox owns new dependency-policy or compatibility decisions. Queue owns the worker, independent review, merge, bot-PR disposition, and lifecycle closeout.

## Boundaries

Follow the task card's exact target versions, owned paths, exclusions, and stop conditions. Keep manifests and direct-version upgrades unchanged. Do not edit workflows, tag, run a release, or close bot PRs before the aggregate PR merges. Preserve the 0.13.0 workspace version and unrelated main changes.

## Important Context

Read `AGENTS.md`, `docs/contracts/001-working-rules.md`, the task card, and the six bot diffs. Targeted Cargo resolution may move resolver-required transitives; review every package delta. Do not repeat the old PR diffs' removal of historical unused patches as a new policy change. Record solvable friction in `PAPERCUTS.md`.

## Suggested Next Move

Reproduce the baseline package inventory, apply the six precise updates to a fresh branch, inspect the lockfile delta, then run the task's focused and full validation as one coherent batch. Open one non-draft aggregate PR linking all six bot PRs.

## Completion Protocol

Obtain independent exact-head review and green current-base CI before Queue merges the aggregate PR. Only after merge, close the six named bot PRs as superseded with the merge link. Record package versions, validation, reviewed head, merge, and dispositions in task evidence. Let the lifecycle hook close the task and remove this handoff; its terminal state unlocks `g10.015`.
