# 545 - Extract Container Exec Colima Runtime Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting container exec by moving Colima runtime detection, profile
inspection, recovery, and reset helpers into a focused module.

## Scope

- create `crates/effigy-containers/src/exec/colima_runtime.rs`
- move Colima-owned helpers where dependencies stay clean:
  - runtime running and ensure-running probes
  - Colima status/profile listing and warnings
  - Colima runtime repair, recovery, and reset
  - backend label and runtime backend state helpers where they are tightly tied
    to Colima selection
- keep public exec facade exports stable
- preserve current recovery behavior and messages

## Non-Goals

- no backend manager behavior changes
- no Docker/Colima CLI invocation redesign
- no container command runner migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when Colima runtime helpers are out of
`exec/implementation.rs`, public exec APIs still compile, and focused recovery
tests still pass or equivalent compile checks cover the mechanical split.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-colima-check cargo check -p effigy-containers`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-colima-libcheck cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-colima-test-a cargo test -p effigy-containers default_runtime_profile_honors_user_global_preference -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Close the effective container policy decomposition lane and update the package
map.
