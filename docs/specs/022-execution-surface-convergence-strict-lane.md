# 022 Execution Surface Convergence Strict Lane

Status: active
Updated: 2026-05-01
Roadmap: `g03.009`

## Context

Effigy now has the first convergence planning and one bounded shipped slice:

- execution-surface responsibility contract
- shared embedded repo-targeting spine

What it does not have yet is shared runtime activation across the remaining
container-backed execution surfaces.

This lane owns the next bounded implementation path for:

- `effigy exec`
- exec aliases
- named-container exec activation
- shared non-shell runtime activation semantics

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/009-execution-surface-convergence.md`
- `docs/roadmaps/g03/009-execution-binding-and-runtime-activation-convergence.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- shared runtime activation for explicit exec
- shared runtime activation for exec aliases
- activation parity with container-backed task execution
- proof that stopped runtime plus `exec` no longer behaves like a special case

This lane does not yet own:

- interactive session ownership convergence
- bootstrap/Rhai/run-array embedded-runner convergence beyond repo targeting
- parity matrix and drift guards

## Current Posture

`strict-ready`

The correct implementation order is:

1. move explicit `exec` and exec aliases onto the shared activation contract
2. prove named-container and default dev-container exec parity
3. decide whether activation convergence is strong enough to move on to
   interactive ownership

## Integration Constraint

- keep this batch focused on non-shell activation
- keep command construction and output shaping surface-specific where needed
- do not fold interactive shell lifecycle work into this lane
- prefer shared activation helpers over new exec-local branching

## Continuation Chain

1. `324` — implement exec activation convergence foundation
2. later — decide post-exec-activation widening

## Exit Condition

This strict lane is complete when:

- stopped runtime plus `effigy exec` behaves consistently with stopped runtime
  plus container-backed task execution
- named-container and default dev-container exec share the same activation
  contract once targeted
- remaining task-versus-exec differences are limited to command/projection
  semantics rather than lifecycle effects

## Next Task

Execute `324` — implement the first bounded exec activation convergence batch.
