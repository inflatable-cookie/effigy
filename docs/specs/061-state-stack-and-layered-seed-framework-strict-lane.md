# 061 - State Stack And Layered Seed Framework Strict Lane

Roadmap: [`g04.019`](../roadmaps/g04/019-state-stack-and-layered-seed-framework.md)

Status: Active
Owner: Platform
Created: 2026-05-08

## Purpose

Define the contract and first proof boundary for a standard Effigy state-stack
framework above the shipped artifact substrate.

This lane exists because Acowtancy has exposed the real missing piece: not OCI
transport, but the ordered lifecycle for structure, seed, imported data,
captures, and rebuilds.

## Hard Boundaries

- keep Effigy app-agnostic
- do not move repo-specific transform or conflict logic into Effigy
- keep `artifact kind` separate from `layer role`
- no automatic sync daemon or background replication
- no `.github/workflows/` edits
- no release execution

## Current Ready Card

[`595-implement-state-stack-manifest-and-lineage-plan-foundation.md`](./batch-cards/595-implement-state-stack-manifest-and-lineage-plan-foundation.md)

## Execution Chain

- `593` complete: opened the lane, promoted the initial roadmap and contract
  anchors, and selected the first contract-shaping card
- `594` complete: promoted the phase model, stack manifest, and Acowtancy proof
  boundary
- `595` ready: implement state-stack manifest and lineage plan foundation

## Exit Condition

This lane closes when Effigy has a durable contract for layered seed/migration
state, a bounded first proof loop, and a clear next implementation surface that
does not depend on inventing app-specific semantics during execution.

## Next Task

Card
[`595-implement-state-stack-manifest-and-lineage-plan-foundation.md`](./batch-cards/595-implement-state-stack-manifest-and-lineage-plan-foundation.md).
