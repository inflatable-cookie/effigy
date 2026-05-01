# 008 - Repo Targeting And Embedded Dispatch Spine

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-01
Depends on: 007

## Problem

Repo targeting is still applied in more than one place when Effigy re-enters
itself from inside a task, script, or other embedded surface.

Today that logic is split across paths such as:

- run-array builtin command dispatch
- Rhai `run_effigy_command`
- bootstrap task re-entry
- other internal command replay helpers

That means the same embedded command can still depend on which path injected the
repo override instead of one shared targeting contract.

## Goal

Create one shared repo-targeting spine for embedded Effigy command dispatch.

## Scope

- add one shared internal `RepoTarget` model with:
  - `invocation_cwd`
  - `resolved_root`
  - `repo_override`
  - `targeting_mode`
- add one shared `apply_repo_target_to_embedded_command(...)` helper
- remove duplicated per-surface repo-override allowlists and match blocks
- make embedded command targeting resolve exactly once
- keep direct CLI dispatch behavior unchanged unless it currently disagrees with
  the shared targeting contract

## Non-Goals

- runtime activation convergence beyond what repo targeting needs
- broader output projection cleanup
- changing user-facing selector or parsing rules

## Exit Condition

This milestone is complete when the same embedded Effigy command resolves the
same repo target whether it is invoked from:

- run-array builtins
- Rhai
- bootstrap
- or direct CLI re-entry

and no duplicated repo-targeting match blocks remain outside the shared helper.

## Outcome

This milestone is now complete.

The shared embedded repo-targeting helper now lives under:

- [`../../../src/runner/command_context/repo_override.rs`](../../../src/runner/command_context/repo_override.rs)

The first callers moved onto it are:

- [`../../../src/runner/execute/sequence_run.rs`](../../../src/runner/execute/sequence_run.rs)
- [`../../../src/runner/script_command.rs`](../../../src/runner/script_command.rs)

The helper now owns the deliberate difference between:

- force-overriding repo targeting for embedded builtin dispatch
- defaulting repo targeting only when the nested command did not already
  declare `--repo`

## Next Task

Promote `g03.009`.

Use the shared repo-targeting spine as the basis for one binding-resolution and
runtime-activation contract across explicit tasks, deferred work, exec, and
shell-backed surfaces.
