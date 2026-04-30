# 003 - Decodelabs Production Strategy Scope

Generation: `g03`

Status: Planned
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

- document the current dedicated-server assumptions
- identify which parts belong in the neutral deployment model
- identify which parts stay manually owned for now
- decide whether a later managed-host path should target the same provider
  adapters as Underlay or a different export track

## Exit Condition

This milestone is complete when Decodelabs has a scoped future strategy and
the Underlay-first deployment work is not blocked by Decodelabs-specific
deployment habits.

## Next Task

Leave this lane planned until the Underlay export path is real enough to reuse
or deliberately reject for Decodelabs.
