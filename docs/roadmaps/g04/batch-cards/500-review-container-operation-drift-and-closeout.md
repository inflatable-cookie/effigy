# 500 - Review Container Operation Drift And Closeout

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Review remaining container-operation drift after manager-backed migration and
decide whether `g04.004` can close.

## Scope

- inventory remaining direct compose/backend calls
- separate allowed adapter-owned helpers from remaining runner/runtime drift
- decide one next move:
  - close `g04.004`
  - migrate remaining volume/runtime helper calls
  - create a deferred cleanup card for `g04.008`
- update lane and roadmap front doors

## Non-Goals

- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `g04.004` is closed or one final implementation card
is ready.

## Closeout

`g04.004` should not close yet.

The review found two remaining runner-owned backend construction paths:

- captured exec still builds its compose prefix with `compose_args`
- runtime volume/reset helpers still call runtime backend invocation directly

Adapter-owned matches remain in `effigy-runtime`, `effigy-containers`, and
`effigy-container-manager`; those are acceptable for this milestone.

## Validation

- `cargo test -p effigy-container-manager`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Remove the final runner-owned compose/runtime helper calls.
