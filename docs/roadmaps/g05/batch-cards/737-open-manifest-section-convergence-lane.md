# 737 - Open Manifest Section Convergence Lane

Roadmap: [`../017-manifest-section-schema-owner-convergence.md`](../017-manifest-section-schema-owner-convergence.md)
Strict lane: [`../../../specs/082-manifest-section-schema-owner-convergence-strict-lane.md`](../../../specs/082-manifest-section-schema-owner-convergence-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-14

## Purpose

Open the `g05.017` implementation lane with a bounded ready chain for the first
canonical `[manifest]` owner slice.

## Scope

- create the active strict lane for `g05.017`
- sequence the ready cards for canonical manifest-section extraction and reuse
- make the next implementation slice explicit in front-door state

## Acceptance

- strict lane `082` exists and is active
- `738` is the first ready implementation card
- active planning front doors point at the lane correctly

## Next Task

Execute `738` to extract the canonical `[manifest]` section owner.
