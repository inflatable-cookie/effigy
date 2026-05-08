# 549 - Move Runtime Image Cleanup Capture Behind Signal Helper

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Remove the remaining inline runtime command capture from image cleanup in
`effigy-runtime`.

## Scope

- add or reuse a signal/runtime helper that captures a
  `ContainerRuntimeInvocationPlan`
- make generated-image cleanup use that helper instead of inline
  `std::process::Command`
- keep missing-image tolerance and error rendering unchanged
- preserve manager invocation plan construction

## Non-Goals

- no image cleanup behavior changes
- no lifecycle reset/down behavior changes
- no public CLI changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `write.rs` no longer owns inline process capture for
runtime image cleanup and focused runtime/lib checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-image-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-image-libcheck cargo check -p effigy --lib`
- PASS:
  `rg 'std::process::Command::new|Command::new' crates/effigy-runtime/src/write.rs`
  returned no matches
- PASS: `git diff --check`

## Next Task

Remove unused args-based compose signal helpers so runtime signals stay
plan-first.
