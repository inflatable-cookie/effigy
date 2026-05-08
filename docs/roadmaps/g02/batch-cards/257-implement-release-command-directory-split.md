# 257 Implement Release Command Directory Split

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Split `src/runner/release_command.rs` into a module directory so release
dispatch, interactive review flow, and thin `effigy-release` adapters stop
living in one 1.2k-line file.

## Context

The release crate extraction is already real. What remains in the runner is
mostly shell and orchestration, but it is still packed into one file with
three distinct layers:

- command dispatch and output policy
- interactive prepare/execute/resume review flow
- release-context, gate, verify-install, and progress helpers

This card turns that mixed shell into smaller runner-owned modules without
reopening the already-set crate boundary.

## In Scope

- Convert `src/runner/release_command.rs` into `src/runner/release_command/`.
- Split the file into local modules such as:
  - dispatch
  - interactive
  - ops
- Further split interactive flow by mode if that keeps the modules small and
  obvious.
- Keep behavior, text output, JSON output, and release protocol rules
  unchanged.

## Out Of Scope

- Any release feature change.
- Workflow edits under `.github/workflows/`.
- Reopening the `effigy-release` crate boundary.

## Acceptance Criteria

- `src/runner/release_command.rs` no longer exists as one monolithic file.
- The runner-side release shell is split into focused local modules.
- Output and behavior stay unchanged.
- Standard validation round passes for the batch.

## Next Task

Card `258` — split the smaller but still mixed `container_command` shell.
