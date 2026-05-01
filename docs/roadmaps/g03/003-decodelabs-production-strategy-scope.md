# 003 - Decodelabs Production Strategy Scope

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-04-30
Depends on: 001, 002

## Problem

Decodelabs deployment still lives in a dedicated-server shape that should not
be flattened into fake managed-platform automation before the strategy is
clear.

## Goal

Keep Decodelabs deployment honest in the short term while scoping what a
future managed-host export strategy would actually need.

## Scope

- inventory the current dedicated-server operating shape without pretending the
  existing local-dev bundle already equals production
- define the short-term truth for Effigy:
  - what `deploy model` / `deploy export` should and should not claim for
    Decodelabs now
  - which production concerns remain operator-owned and manual
- identify the first reusable production abstractions that really do belong in
  the neutral deployment model
- decide whether a future Decodelabs export path should target:
  - a dedicated-host export track
  - the same managed-provider adapters as Underlay
  - or a split strategy

## Current Focus

`g03.001` and `g03.002` are closed. Underlay export is now real enough that
Decodelabs can be handled on its own terms instead of blocking the deployment
lane.

The immediate planning target is not provider automation. It is strategy truth:

- what the current Decodelabs production story actually is
- what Effigy should explicitly refuse to claim yet
- what the first real reusable production contract would be

## Current Findings

The first inventory pass now says:

- Decodelabs production is still dedicated-host-first
- the local bundle is not a trustworthy production topology proxy
- older Deploy/Effigy behavior is mainly:
  - `git pull`
  - `composer install --no-dev`
  - optional app-owned build
- parts of the broader legacy estate still use host-specific release flows,
  including Windows/IIS notes
- queue/background work exists in app code, but there is no shared repo-level
  production supervisor shape to promote yet

## Decision

`333` closes this lane on an explicit short-term answer:

- keep Decodelabs production operator-owned for now
- do not widen into provider export
- do not widen into dedicated-host export yet

The inventory was useful, but it did not expose one clean production topology
that Effigy can honestly emit.

## Promoted Anchors

- [`../../architecture/021-production-deployment-export-architecture.md`](../../architecture/021-production-deployment-export-architecture.md)
- [`../../contracts/010-decodelabs-production-strategy.md`](../../contracts/010-decodelabs-production-strategy.md)

## Exit Condition

This milestone is now complete.

Decodelabs has an explicit short-term production posture:

- Effigy owns local-dev support strongly
- Effigy does not yet own Decodelabs production export
- any later widening must be triggered by a real converged topology or a real
  cross-bundle deployment need

## Next Task

Leave this roadmap closed.

If Decodelabs production work becomes real again, reopen it from a concrete
topology or promotion trigger instead of generic “future strategy” language.
