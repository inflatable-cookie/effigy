# 084 Decide Demo Release Execution Readiness

Status: archived
Updated: 2026-04-13
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Decide whether Effigy should move from bounded release prep into actual release
execution work for the shipped demo surface once the working tree is clean and
a human explicitly requests release execution.

## In Scope

- assess the release-readiness checkpoint produced in `083`
- confirm whether the remaining risks are acceptable for a `0.2.13` release:
  - only one validated consumer repo before release prep
  - consumer-local script/runtime issues outside Effigy itself
  - current working-tree cleanliness requirements before release execution
- choose one explicit next step:
  - release execution work under the release protocol
  - or one more bounded pre-release fix / validation batch

## Out Of Scope

- actual release execution itself
- workflow edits
- further browser or runner feature work unless a true release blocker appears

## Acceptance Criteria

- the lane records whether actual release execution work is justified next
- any remaining blocker is explicit and bounded
- the next ready card is unambiguous

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the decision drifts into running release commands without explicit human ask
- the batch hides the clean-worktree requirement for release execution

## Decision

- actual release-execution work is justified next
- no additional demo-surface validation or implementation batch is required
  first
- preconditions remain explicit:
  - working tree must be clean
  - a human must explicitly ask for release execution

## Outcome

- ready card opened:
  [`085-execute-demo-release-protocol.md`](./085-execute-demo-release-protocol.md)

## Next Task

Execute [`085-execute-demo-release-protocol.md`](./085-execute-demo-release-protocol.md)
once the working tree is clean and a human explicitly asks to run the release
protocol.
