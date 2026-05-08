# 403 - Add Inside Container Reentry Context Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Prove inside-container task re-entry keeps the captured runtime context stable.

## Scope

- add or tighten a focused runtime/execution proof
- simulate an inside-container handoff context
- assert container-targeted execution resolves locally when already inside the
  container handoff
- assert repo/cwd path authority comes from the captured context, not fresh env
  probing

## Exit Condition

This card is complete when the proof fails if inside-container re-entry starts
guessing host/container state or path authority again.

## Closeout

Tightened the `effigy-execution` inside-container handoff proof.

The proof now simulates a nested cwd inside a repo while the captured runtime
context marks the process as inside an Effigy container handoff. A
container-targeted command resolves to `LocalContainerHandoff`, and the resolved
plan keeps:

- captured invocation cwd
- captured command/root target
- captured container handoff state

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-execution handoff_routes_locally -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-execution -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next Task

Add the manager operation-report identity proof.
