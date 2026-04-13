# 082 Decide Demo Release Readiness After Signal Proof

Status: ready
Updated: 2026-04-13
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Decide whether the shipped demo browser and runner surface is honest enough to
leave the strict lane and enter release prep after the bounded Signal proof
pass, or whether one more real consumer validation batch is still required.

## In Scope

- assess the real-project proof actually gathered in `081`
- weigh the release significance of Signal as the proving consumer repo
- make any remaining release-risk gaps explicit:
  - second consumer proof still missing
  - consumer repo script/runtime issues versus Effigy product issues
  - remaining browser/demo ergonomics that are release-blocking versus
    release-follow-up
- choose one explicit next step:
  - release-prep / release-decision work
  - one more bounded consumer validation slice

## Out Of Scope

- release execution itself
- new browser polish or runner implementation unless the decision proves a
  blocking product gap
- broad consumer repo migration work

## Acceptance Criteria

- the lane records whether Signal-only proof is sufficient for this release
  boundary
- any missing second-consumer proof is classified explicitly as:
  - release-blocking
  - or accepted residual risk
- the next ready card is unambiguous

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the decision starts drifting into release execution steps instead of boundary
  clarity
- the lane tries to hide missing consumer proof behind vague “good enough”
  language

## Next Task

Execute this decision batch next, then leave one explicit release-prep or
extra-consumer-validation card instead of free-continuing.
