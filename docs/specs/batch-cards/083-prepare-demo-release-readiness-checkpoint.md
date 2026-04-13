# 083 Prepare Demo Release Readiness Checkpoint

Status: ready
Updated: 2026-04-13
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Prepare the demo-surface release-readiness checkpoint now that the strict lane
has enough real consumer proof to leave implementation and enter bounded
release prep.

## In Scope

- summarize the shipped demo surface honestly for release-readiness review:
  - registry and manifest composition
  - inspect/history/run/stop/rerun
  - browser, live terminal, color, and input behavior
  - Signal consumer proof
- record explicit residual risks:
  - only one real consumer repo validated before release prep
  - known consumer-local script/runtime issues that do not block Effigy itself
- recommend whether release execution should proceed once normal release gates
  are satisfied
- update currentness surfaces so the strict lane points at release-prep rather
  than more implementation

## Out Of Scope

- release execution itself
- more product implementation unless the checkpoint proves a real blocker
- broad extra consumer migrations

## Acceptance Criteria

- one release-readiness checkpoint log exists for the demo surface
- residual risks are explicit and operator-visible
- the next ready card cleanly hands off to release-prep / release-decision work

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch drifts into actual release execution
- the checkpoint hides residual risk behind generic “ready” language

## Next Task

Execute this release-prep checkpoint batch next, then leave one explicit
release-prep or release-decision card instead of drifting back into product
implementation.
