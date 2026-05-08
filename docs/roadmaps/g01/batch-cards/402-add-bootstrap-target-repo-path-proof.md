# 402 - Add Bootstrap Target Repo Path Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Prove bootstrap task execution keeps the cloned target repo as path authority
instead of drifting to the invocation cwd.

## Scope

- add or tighten a focused bootstrap proof
- use a synthetic remote/target repo only
- assert bootstrap setup/start task execution writes into the target repo
- assert invocation cwd does not become the effective repo root for embedded
  task execution
- avoid live external repos or network

## Exit Condition

This card is complete when the bootstrap proof fails if target repo path
authority drifts back to invocation cwd.

## Closeout

Tightened the full bootstrap CLI integration proof. The synthetic root, child,
and start bootstrap tasks now record their process cwd, and the test asserts:

- root setup runs in the cloned root target repo
- child setup runs in the cloned child target repo
- start runs in the cloned root target repo
- no task markers are written into the invocation cwd

Also refreshed a stale bootstrap help assertion exposed by the full bootstrap
integration test run.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test --test bootstrap_cli_tests bootstrap_executes_root_child_and_start_via_binary -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test --test bootstrap_cli_tests -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Add the inside-container re-entry context proof.
