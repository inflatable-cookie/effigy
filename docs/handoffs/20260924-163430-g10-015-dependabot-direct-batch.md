---
kind: northstar-handoff
title: "Effigy g10.015 — Resolve direct Dependabot upgrades"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
base_required: pushed-main
queue_dispatch: northstar-queue
queue_approval: "Operator asked to resolve all open Dependabot PRs and confirmed consolidated reviewed batch PRs on 2026-09-24."
queue:
  capability: complex
  dependsOn: [b3bb9d32-7614-4acf-8d31-92f1ec909462]
---

## What This Thread Was Doing

The operator wants the ten open Dependabot PRs resolved in consolidated, reviewed current-base batches. Implement [g10.015](../roadmaps/g10/015-resolve-direct-dependabot-upgrades.md) only after Queue records terminal closeout for `g10.014`. The task card owns scope and acceptance.

## Why It Matters

The remaining bot PRs update three directly used crates and one overlapping transitive crate. Compilation alone cannot prove existing vaults, graph facts, or table output survive these upgrades. One current-base replacement PR makes the compatibility proof reviewable.

## Current State

- Queue prerequisite: `g10.014`, task `b3bb9d32-7614-4acf-8d31-92f1ec909462`. Queue must hold this lane until that task is done, including merge and lifecycle closeout.
- Source bot PRs: #81 (`argon2`), #98 (`tree-sitter`), #101 (`tree-sitter-language`, included with #98), and #102 (`tabled`). Their old green checks do not prove the aggregate against current `main`.
- Start from the reviewed `g10.014` merge and record its exact lockfile baseline. Queue creates an isolated worker workspace.
- Chatterbox owns new compatibility or migration decisions. Queue owns the worker, independent review, merge, bot-PR disposition, and lifecycle closeout.

## Boundaries

Follow the task card's manifests, focused code/tests, exclusions, and stop conditions. Preserve existing vault derivation, graph facts, and text table bytes. Do not silently introduce a vault migration, graph schema change, or table redesign. Do not edit workflows, tag, run a release, or close bot PRs before the aggregate merge.

## Important Context

Read `AGENTS.md`, `docs/contracts/001-working-rules.md`, the task card, and the four bot diffs. `crates/effigy-secrets` derives raw Argon2id keys with fixed parameters; capture a baseline vector from `v0.13.0` before changing the dependency. Graph indexers use Tree-sitter across Rust, JS/TS, Python, and PHP. Plain table rendering lives in `crates/effigy-ui/src/table.rs`. Record solvable friction in `PAPERCUTS.md`.

## Suggested Next Move

After Queue releases the dependency hold, verify the merged `g10.014` baseline, capture old-vault/graph/table compatibility fixtures, apply the exact direct upgrades and lockfile resolution, then run the task's focused and full validation. Open one non-draft aggregate PR linking the four bot PRs.

## Completion Protocol

Obtain independent exact-head review and green current-base CI before Queue merges the aggregate PR. Only after merge, close the four named bot PRs as superseded with its merge link. Record version/transitive changes, compatibility vectors, validation, reviewed head, merge, and source-PR dispositions. Let the lifecycle hook close the task and remove this handoff; then return to Chatterbox planning.
