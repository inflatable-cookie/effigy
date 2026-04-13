# 082 Decide Demo Release Readiness After Signal Proof

Status: complete
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

## Decision

- Signal-only real-consumer proof is sufficient to let `g02.003` leave strict
  implementation and enter release prep
- the missing second-consumer proof is accepted as explicit residual release
  risk, not as a hidden blocker
- reason:
  - Signal exercises the real browser, history, live terminal, and inline-demo
    manifest surfaces on a non-Effigy consumer repo
  - the remaining issues discovered in that proof window were consumer-local
    script/runtime problems rather than Effigy product failures
  - release prep now has enough real evidence to proceed honestly as long as it
    records the missing second-consumer proof as follow-up risk

## Outcome

- release-prep ready card opened:
  [`083-prepare-demo-release-readiness-checkpoint.md`](./083-prepare-demo-release-readiness-checkpoint.md)

## Next Task

Execute [`083-prepare-demo-release-readiness-checkpoint.md`](./083-prepare-demo-release-readiness-checkpoint.md)
to assemble the demo-surface release-readiness checkpoint, residual risks, and
operator recommendation before any actual release execution work.
