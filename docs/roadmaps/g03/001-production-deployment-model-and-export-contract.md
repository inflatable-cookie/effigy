# 001 - Production Deployment Model And Export Contract

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-04-30
Depends on: 011, 013, 016, 021

## Problem

Effigy knows enough about app structure to generate real deployment artifacts,
but there is no provider-neutral production model yet.

Without that layer, any Render or Railway export would either:

- mirror local dev too literally
- or bury product logic inside one provider adapter

Both are the wrong boundary.

## Goal

Define the production deployment model and command contract that future export
adapters consume.

## Scope

- one provider-neutral deployment model
- one inspectable command surface for that model
- one export report/warnings contract
- one clear file/template ownership rule for provider adapters
- Underlay-first proof boundary

## Non-Goals

- live provisioning
- secret sync
- one-click deploy
- full Decodelabs production automation

## Exit Condition

This milestone is complete when Effigy has a stable, inspectable deployment
model and an export command contract strong enough that provider work can stay
thin and testable.

## Outcome

Delivered:

- `deploy.model.v1`
- Underlay derivation
- `deploy model --json`
- Render export foundation plus real Underlay proof
- Railway export foundation plus real Underlay proof

## Next Task

`g03.001` is complete. Re-sequence the next milestone deliberately instead of
keeping the old strict lane open by inertia.
