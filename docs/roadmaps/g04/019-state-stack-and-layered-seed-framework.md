# 019 - State Stack And Layered Seed Framework

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-08
Depends on:
- [`018-oci-artifact-closeout-and-proof-matrix.md`](./018-oci-artifact-closeout-and-proof-matrix.md)

## Goal

Turn Effigy's seed/apply/capture substrate into a standard state-stack
framework for schema baselines, replayable imported data, layered overlays, and
capture/rebase workflows.

The trigger is Example App. OCI transport is now real, but the harder problem is
composing system state over time without pushing every repo into bespoke
migration orchestration.

## Scope

- define a phase taxonomy for structure, baseline seed, imported data, dev
  overlays, captures, refreshes, rebases, and full snapshots
- promote a state-stack contract that keeps layer role separate from artifact
  kind
- define the manifest/report surface Effigy should own for ordered layer replay
- define the app-hook boundary for apply/capture work without moving
  repo-specific migration logic into Effigy
- define lineage/provenance rules so operators can see what exact stack built an
  environment
- model the Example App UAT freeze, capture, refresh, and rebuild loop as the
  first proof target

## Non-Goals

- no MySQL-to-Postgres transform engine in Effigy
- no record-level merge or conflict resolution logic
- no media rewrite semantics inside Effigy
- no live legacy-to-new sync daemon
- no automatic publish behavior
- no release execution
- no `.github/workflows/` edits

## Why Now

The artifact substrate closed the transport problem:

- local and OCI payloads resolve through one staging model
- seed and dump flows already consume that substrate
- capture/push already has an explicit operator contract

What is still missing is the orchestration model above those pieces.

Without that layer, every consumer repo still has to reinvent:

- what phase comes before what
- which layers are reusable versus local-only
- how UAT-created changes are captured
- how refreshes and rebuilds stay auditable
- where app logic ends and Effigy logic starts

## Proposed Surface

Effigy should add a repo-declared state stack, not a generic migration engine.

Core ideas:

- `state stack`
  - ordered description of how a clean system becomes a working seeded system
- `layer role`
  - orchestration meaning such as `structure`, `baseline-seed`,
    `legacy-import`, `overlay`, `uat-capture`, `full-capture`
- `artifact kind`
  - coarse payload classification such as `sql-dump`, `content-overlay`, or
    `app-specific`
- `lineage`
  - immutable record of what layers, refs, digests, and hooks built a given
    environment

Likely future command family:

```sh
effigy state plan
effigy state apply
effigy state capture
effigy state lineage
```

## Example App Proof

The first proof is not post-go-live sync.

The first proof is the repeatable UAT loop:

1. apply schema and baseline seed
2. apply legacy-import artifacts
3. optionally apply dev-only overlays
4. hand off a working UAT system
5. freeze UAT
6. capture UAT-authored changes as a replayable overlay
7. refresh legacy-import artifacts from a newer old-site snapshot
8. reconcile offline
9. rebuild a fresh baseline and redeploy

That loop is general enough for Effigy. The reconciliation logic inside it is
not.

## Acceptance Criteria

- a durable contract defines the phase taxonomy and state-stack boundary
- the stack manifest/report model is explicit enough to implement without
  smuggling app semantics into Effigy
- Example App's UAT freeze/rebase loop is modeled as the first proof case
- the first implementation card is narrow and proves manifest/lineage planning
  before app hook execution

## Result

This roadmap is complete for the first release boundary.

Effigy now owns the app-agnostic state-stack frame: composed or standalone stack
declaration, ordered layer planning, bounded apply adapters, explicit capture
orchestration, and file-backed report history. The Example App proof confirms
that app-owned transform and reconciliation logic can sit behind this frame
without being absorbed into Effigy.

Hold the next release here. Further work should wait for real Example App rebase
pressure against the released contract.

## Validation

- docs path/link checks for changed planning docs
- `git diff --check`

## Next Task

Hand off to the release-prep thread. Release execution remains human-owned.
