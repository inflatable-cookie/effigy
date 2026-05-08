# 297 Decide Post Readiness Foundation Follow-Up

Status: archived
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Choose the next bounded `g02.013` batch now that managed dev tasks can own
container lifecycle, an embedded shell, and one honest readiness message.

## Scope

- assess the remaining `g02.013` gaps against the shipped lifecycle, shell,
  and readiness foundation
- decide whether gateway auto-start or real-project proof is the next bounded
  batch
- refresh the front-door planning surfaces so `continue` resolves to one
  explicit execution card

## Out Of Scope

- implementing the follow-up batch itself
- broad `effigy dev` closeout beyond the next bounded slice
- widening multiple `g02.013` concerns at once

## Acceptance

- one explicit next execution card exists for `g02.013`
- the chosen batch stays on already-shipped substrate
- the lane front doors stop pointing at `296`

## Decision

The next `g02.013` batch should be gateway auto-start, not real-project proof.

Why gateway comes first:

- `296` already covers the last missing local-runtime feedback seam, so the
  next product gap is still startup friction: the developer should not have to
  start the gateway out-of-band after the managed dev loop is already running
- the gateway lane already ships host-native startup, status, and DNS/TLS
  registration on the bounded product path, so auto-start is follow-through on
  shipped substrate rather than a fresh subsystem invention
- the roadmap's target envelope already names gateway auto-start before the
  final real-project proof, and the proof should exercise the fuller daily
  driver loop instead of freezing evidence one batch early

What stays out of the next batch:

- broader gateway lifecycle orchestration beyond one bounded auto-start path
- real-project proof and lane closeout

## Result

The next explicit `g02.013` execution batch is now card `298`.

## Next Task

Execute `298` to land the managed dev gateway auto-start foundation.
