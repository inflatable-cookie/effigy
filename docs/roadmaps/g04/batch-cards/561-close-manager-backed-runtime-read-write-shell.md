# 561 - Close Manager Backed Runtime Read Write Shell

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Close `g04.008` after the runtime read/write/shell/data split.

## Scope

- run the focused runtime drift inventory
- confirm runtime `compose_args` usage is limited to named adapter seams
- confirm old Docker-named runtime helper exports remain removed
- update `g04.008` as complete if the acceptance criteria are met
- select the next roadmap, expected `g04.009`

## Non-Goals

- no new runtime code movement
- no runner-wide drift cleanup
- no CLI parser changes yet
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `g04.008` is marked complete, the focused runtime
drift inventory is recorded, and the next ready card points at `g04.009`.

## Validation

- PASS: `rg -n 'compose_args\(' crates/effigy-runtime/src`
  - only `crates/effigy-runtime/src/container_manager.rs`
- PASS:
  `rg -n 'run_docker_capture|resolve_compose_backend|ComposeBackend' crates/effigy-runtime/src`
  returned no matches
- PASS:
  `rg -n 'Command::new\("(docker|colima|nerdctl)"' crates/effigy-runtime/src`
  returned no matches
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-shell-exec-args-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-shell-exec-args-libcheck cargo check -p effigy --lib`
- PASS: `git diff --check`

## Next Task

Start CLI parser modularisation for runtime surfaces.
