# 003 - Decodelabs Production Strategy Scope

Generation: `g03`

Status: Active
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

## Promoted Anchors

- [`../../architecture/021-production-deployment-export-architecture.md`](../../architecture/021-production-deployment-export-architecture.md)
- [`../../contracts/010-decodelabs-production-strategy.md`](../../contracts/010-decodelabs-production-strategy.md)

## Exit Condition

This milestone is complete when:

- Decodelabs has an explicit short-term production posture
- Effigy’s deployment surface is honest about what it does not support yet for
  Decodelabs
- the future export direction is narrowed enough that later work is sequencing,
  not rediscovery

## Next Task

Decide the post-inventory boundary:

- what small neutral-model subset, if any, should be promoted next
- whether Decodelabs wants a dedicated-host export track
- or whether the honest short-term answer is to keep Decodelabs explicitly
  operator-owned
