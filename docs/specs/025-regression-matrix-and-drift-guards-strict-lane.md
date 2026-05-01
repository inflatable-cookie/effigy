# 025 Regression Matrix And Drift Guards Strict Lane

Status: active
Updated: 2026-05-01
Roadmap: `g03.012`

## Context

Effigy now has the main execution-surface convergence slices landed:

- shared embedded repo-targeting spine
- shared non-shell activation
- shared interactive ownership foundation for direct workspace and seeded task
  shells
- shared embedded-runner foundation for Rhai, run-array builtins, and
  bootstrap task dispatch

The next highest-value work is no longer another convergence refactor. It is
proving those seams and guarding them against drift.

Without a focused parity matrix and a few explicit audit guards, the same class
of regression can return under a different entrypoint and stay invisible until
one consumer repo trips it.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/009-execution-surface-convergence.md`
- `docs/roadmaps/g03/012-regression-matrix-and-drift-guards.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- a focused execution-surface parity matrix
- drift guards for shared repo targeting and embedded command replay
- explicit proof for deliberate unsupported-surface exceptions
- enough fixture coverage to catch caller-path regressions before consumers do

This lane does not own:

- new convergence refactors unless the tests expose a fresh break
- widening the matrix into a giant end-to-end smoke suite
- new deployment-export work

## Current Posture

`active`

The correct implementation order is:

1. codify the minimum parity scenarios from the convergence contract
2. add one explicit audit guard around shared embedded repo targeting
3. prove the intentional exception families so they stop drifting silently
4. close the remaining runtime side-effect parity gaps before pausing the lane

## Integration Constraint

- keep this lane test- and guard-heavy, not architecture-heavy
- prefer focused parity tests over broad smoke suites
- treat newly exposed drift as follow-up work, not an excuse to widen this lane
  in place unless the break is immediate and obvious

## Continuation Chain

1. `329` — implement the first convergence parity matrix foundation
2. `330` — decide whether another bounded drift-guard slice is needed
3. `331` — implement runtime side-effect parity closeout

## Exit Condition

This strict lane is complete when the convergence program has executable proof
that:

- common lifecycle effects do not depend on caller path
- deliberate exceptions are visible and tested
- new split-path regressions are caught by contract-anchored tests

## Next Task

Execute `331` — close the remaining runtime side-effect parity gaps before
pausing `g03.012`.
