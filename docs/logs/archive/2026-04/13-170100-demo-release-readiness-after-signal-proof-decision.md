# Demo Release Readiness After Signal Proof Decision

Date: 2026-04-13
Roadmap: `g02.003`
Card: [`082-decide-demo-release-readiness-after-signal-proof.md`](../../../specs/batch-cards/082-decide-demo-release-readiness-after-signal-proof.md)

## Summary

Decided that the shipped demo surface has enough real consumer proof to leave
strict implementation and enter release prep, using Signal as the proving
consumer and keeping the missing second-consumer proof explicit as residual
risk.

## Vision Target Delta

- Tags: `DEMO`, `CONTRACT`, `OPERATE`
- Moved: `strict-lane consumer proof still ambiguous for release prep` ->
  `release-prep entry approved with one real consumer proof and explicit
  residual risk`
- Remaining: prepare the release-readiness checkpoint and operator
  recommendation before any release execution work

## Decision

- proceed to release prep
- do not require one more consumer-validation batch before release prep
- classify missing second-consumer proof as accepted residual risk, not a
  release-blocking gap

## Why

- Signal now proves the key shipped demo surfaces on a real non-Effigy repo:
  - included manifest composition
  - native demo registry
  - inline demo run sequences
  - browser discovery/detail/history/live terminal flow
- the issues found in that proof window were either fixed in Effigy already or
  were consumer-local script/runtime issues outside the Effigy product boundary
- forcing a second consumer batch now would add schedule churn without changing
  the core product truth already demonstrated

## Residual Risk

- a second real consumer repo was not completed before release prep
- some consumer repos may still need local script/runtime cleanup after adopting
  the new demo surface

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Outcome

Opened ready card
[`083-prepare-demo-release-readiness-checkpoint.md`](../../../specs/batch-cards/083-prepare-demo-release-readiness-checkpoint.md).

## Next Task

- Execute `083-prepare-demo-release-readiness-checkpoint.md`
