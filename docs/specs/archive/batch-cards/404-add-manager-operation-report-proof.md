# 404 - Add Manager Operation Report Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

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

## Closeout

Tightened the non-status manager operation-report proof in
`crates/effigy-container-manager`.

The lifecycle report proof now asserts:

- backend id
- interrupt policy name
- repo root
- lifecycle action
- runtime state
- cleanup failure payload

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-container-manager operation_report -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next Task

Add the direct/bootstrap/Rhai execution-plan parity proof.
