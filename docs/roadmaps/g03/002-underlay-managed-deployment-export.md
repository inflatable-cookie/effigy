# 002 - Underlay Managed Deployment Export

Generation: `g03`

Status: Planned
Owner: Platform
Created: 2026-04-30
Depends on: 001

## Problem

Underlay apps are the best near-term target for production export, but Effigy
does not yet derive or render that shape for managed platforms.

## Goal

Prove the deployment export surface on real Underlay topology first.

## Scope

- derive Underlay web, worker, and backing-service intent into the neutral
  deployment model
- render provider exports for Render and Railway
- generate a report for missing secrets, scaling decisions, and other policy
  gaps
- prove the shape against at least one real Underlay consumer repo

## Exit Condition

This milestone is complete when Effigy can export a coherent Underlay
deployment bundle for at least one managed provider and the translation logic
is clearly owned by the neutral model plus thin provider adapters.

## Next Task

Wait for `g03.001` to land the neutral model, then implement the Underlay
derivation path and first provider template set.
