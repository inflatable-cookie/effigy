# 401 - Add Underlay Generated Compose Path Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Prove Underlay generated-compose path handling against the new runtime/container
foundation.

## Scope

- add or tighten a focused `effigy-containers` fixture for an Underlay-like
  generated compose shape
- assert generated compose lives under `.effigy/runtime/compose`
- assert workspace/root paths stay repo-targeted
- assert external sibling mount mapping remains stable
- keep the fixture synthetic and local to tests

## Exit Condition

This card is complete when the Underlay fixture fails if generated compose path
ownership or external mount mapping drifts.

## Next Task

Add the Underlay generated-compose path proof.
