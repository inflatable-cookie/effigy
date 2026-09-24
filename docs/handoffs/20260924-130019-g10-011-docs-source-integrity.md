---
kind: northstar-handoff
title: "Effigy g10.011 — Cross-repository docs-source integrity"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
base_required: pushed-main
queue_dispatch: northstar-queue
queue_approval: "Operator approved all four release-audit findings as pre-release tasks and dispatch through Queue on 2026-09-24; confirmed target 0.13.0."
queue:
  capability: complex
---

## What This Thread Was Doing

The release-readiness audit found the defect now owned by [g10.011](../roadmaps/g10/011-cross-repository-docs-source-integrity.md). Implement that ready Northstar task through the normal worker PR loop. The task card is the dispatch manifest and acceptance authority.

## Why It Matters

Committed consent, source provenance, unique basename handles, and truthful directory status. The operator requires this before the 0.13.0 release candidate can be ready.

## Current State

- Base: pushed integration `main`; Queue creates the isolated worker workspace.
- Governing and likely owner paths: Contract 041 and guide 079; manifest docs policy, codegraph Git/source paths, docs-context CLI fixtures.
- Independent concurrent siblings: `g10.011`, `g10.012`, and `g10.013`. Do not duplicate sibling work.
- `CHANGELOG.md` is shared; retain all independent entries during serialized merges.
- The audit baseline passed local QA, but exact-head hosted CI has not yet satisfied the release gate.

## Boundaries

Follow the task card's owned paths, exclusions, stop conditions, and escalation owner. Preserve source/consumer isolation and public contracts. Do not edit `.github/workflows/`, run release mutations, change version, or treat this handoff as release authorization. The Chatterbox owns new product-policy decisions; Queue owns worker, review, merge, and hook closeout.

## Important Context

Real Git fixtures for dirty opt-in/out, includes/overlays, quoted and renamed paths; duplicate names fail before query; absent vs unreadable directory status; existing JSON shape. Read the card and governing references before implementation. Add a user-facing `[Unreleased]` changelog entry. Report solvable friction in `PAPERCUTS.md` without diverting this task.

## Suggested Next Move

Reproduce the failure with focused adversarial tests, implement the bounded repair, then validate the full coherent batch. Open one non-draft PR with the task's evidence and limits.

## Completion Protocol

Run the task card's focused and proportionate validation, formatting/Clippy, docs and JSON checks as applicable. Require independent exact-head review, resolve findings on the same PR, merge through Queue, and let the lifecycle hook close the task and remove this handoff. Report the final reviewed head, merge, validation, and residual risks. A successful task does not itself initiate the release.
