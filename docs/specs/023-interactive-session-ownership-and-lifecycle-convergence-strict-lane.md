# 023 Interactive Session Ownership And Lifecycle Convergence Strict Lane

Status: active
Updated: 2026-05-01
Roadmap: `g03.010`

## Context

Effigy now has the first convergence planning and three bounded shipped slices:

- execution-surface responsibility contract
- shared embedded repo-targeting spine
- shared non-shell activation for `exec`, exec aliases, and container-backed
  task execution

What it does not have yet is one shared ownership model for interactive
surfaces.

The remaining drift is concentrated in:

- `effigy workspace`
- managed handoff and adopted-runtime shell entry
- `stay_in_shell`
- attached `container up` session cleanup

Those surfaces still decide teardown, adopted-runtime handling, and readiness
completion through local booleans more often than through one shared session
contract.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/009-execution-surface-convergence.md`
- `docs/roadmaps/g03/010-interactive-session-ownership-and-lifecycle-convergence.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- shared runtime ownership classification for interactive surfaces
- shared session-readiness completion for shell-backed entrypoints
- shared cleanup derivation for adopted versus session-owned runtimes
- convergence of workspace handoff, `stay_in_shell`, and attached-session
  teardown semantics

This lane does not yet own:

- broader bootstrap, Rhai, and embedded-runner convergence
- parity-matrix drift guards
- non-interactive task/runtime activation, already closed in `g03.009`

## Current Posture

`strict-ready`

The correct implementation order is:

1. introduce one shared ownership/readiness model for interactive surfaces
2. apply it to workspace handoff and the main session-owned task paths
3. decide whether attached operator sessions can move onto the same cleanup
   model immediately or need one more bounded follow-up

## Integration Constraint

- keep this lane focused on interactive ownership and cleanup
- do not reopen non-shell activation semantics already closed in `g03.009`
- prefer one shared ownership helper over caller-local booleans
- keep output and TUI differences surface-specific where needed

## Continuation Chain

1. `325` — implement interactive ownership classification foundation
2. later — decide post-foundation widening

## Exit Condition

This strict lane is complete when equivalent interactive sessions produce
equivalent cleanup behavior regardless of whether the runtime was first touched
by:

- bootstrap
- task auto-activation
- explicit `dev`
- workspace handoff

## Next Task

Execute `325` — implement the first bounded interactive ownership and cleanup
convergence batch.
