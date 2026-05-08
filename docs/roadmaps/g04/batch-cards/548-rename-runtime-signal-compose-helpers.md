# 548 - Rename Runtime Signal Compose Helpers

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Remove Docker-specific helper names from runtime signal APIs where the helpers
already route compose/backend behavior through runtime plans.

## Scope

- replace `run_docker_capture` with a compose/backend-neutral runtime signal
  helper name
- replace `spawn_docker_inherit` with a compose/backend-neutral inherited
  process helper name
- migrate runner callers in runtime prep and standard activation
- keep behavior and command invocation shape unchanged
- leave deeper manager API changes for later cards

## Non-Goals

- no lifecycle behavior changes
- no signal handling redesign
- no public CLI changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runtime/runner code no longer references the
Docker-named runtime signal helpers and focused runtime/runner checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-signals-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-signals-libcheck cargo check -p effigy --lib`
- PASS:
  `rg 'run_docker_capture|spawn_docker_inherit' crates/effigy-runtime/src src/runner`
  returned no matches
- PASS: `git diff --check`

## Next Task

Move runtime command capture for image cleanup behind a shared signal helper.
