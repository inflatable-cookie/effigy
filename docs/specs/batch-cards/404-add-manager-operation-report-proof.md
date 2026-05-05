# 404 - Add Manager Operation Report Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Prove manager-backed operation reports carry stable identity and cleanup fields.

## Scope

- add or tighten focused `effigy-container-manager` tests
- assert report identity for backend id, policy name, repo root, action, state,
  and cleanup result
- cover at least one non-status action report used by runner lifecycle paths
- no public CLI JSON schema changes

## Exit Condition

This card is complete when manager operation reports fail tests if identity or
cleanup fields drift.

## Next Task

Add the manager operation-report identity proof.
