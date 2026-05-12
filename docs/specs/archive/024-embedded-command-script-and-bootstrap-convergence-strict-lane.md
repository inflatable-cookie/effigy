# 024 Embedded Command, Script, And Bootstrap Convergence Strict Lane

Status: complete
Updated: 2026-05-01
Roadmap: `g03.011`

## Context

Effigy now has the first convergence planning and four bounded shipped slices:

- execution-surface responsibility contract
- shared embedded repo-targeting spine
- shared non-shell activation for `exec`, exec aliases, and container-backed
  task execution
- shared interactive ownership classification for direct workspace and seeded
  task shells

The next highest-value drift is no longer shell cleanup. It is internal Effigy
re-entry.

Bootstrap dispatch, Rhai command execution, run-array builtins, and adjacent
embedded command paths still carry too many local assumptions around:

- JSON propagation
- recursion and handoff rules
- locking
- nested output shaping

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/009-execution-surface-convergence.md`
- `docs/roadmaps/g03/011-embedded-command-script-and-bootstrap-convergence.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- one shared embedded-runner entry for internal Effigy command re-entry
- shared JSON-mode propagation for nested command replay
- shared recursion and handoff rules for embedded command execution
- convergence of bootstrap, Rhai, and run-array builtin entrypoints onto that
  spine

This lane does not yet own:

- broader regression-matrix drift guards
- redesign of bootstrap planning or Rhai host-surface scope
- new execution binding or interactive ownership work already closed in
  `g03.009` and `g03.010`

## Current Posture

`complete`

The correct implementation order is:

1. introduce one shared embedded-runner entry with stable nested command
   semantics
2. move bootstrap/Rhai/run-array callers onto that entry
3. decide whether any caller-specific projection differences still justify one
   more bounded follow-up before the drift-guard lane

## Integration Constraint

- keep this lane focused on embedded command re-entry
- reuse the already-shipped repo-targeting helper instead of reopening it
- keep runtime semantics delegated to the resolved inner surface
- keep caller-specific presentation differences narrow and explicit

## Continuation Chain

1. `327` — complete; Rhai command replay, run-array builtins, and bootstrap
   task dispatch now share the first embedded-runner spine
2. `328` — complete; bootstrap managed-run synthesis is not a normal embedded
   replay surface, so the lane hands off to drift guards instead of widening

## Exit Condition

This strict lane is complete when one shared embedded command path exists and
bootstrap, Rhai, and run-array builtin execution use it unless a documented
exception still applies.

## Next Task

Promote `g03.012`.
