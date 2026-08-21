# g08.034 - Documentation Coverage Parity

Status: Complete
Depends on: current `main` behavior and completed `g08.033`
Spec: [`107`](../../specs/archive/107-documentation-coverage-parity.md)

## Goal

Make Effigy's active user, agent, built-in, and generated documentation agree
with the current public behavior across the whole repository, then make
mechanically detectable drift fail close to the change that caused it.

## Vision Alignment

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Target envelope: operators and agents can discover current behavior from the
  surface they naturally enter without reconstructing it from source.
- Vision target delta: documentation coverage moves from feature-by-feature
  best effort to an evidence-backed, regression-guarded repository sweep.

## Execution Plan

- [x] card `1086`: inventory current public behavior and repair all verified
      gaps across active user, agent, built-in, and generated docs
- [x] card `1087`: add proportional recurrence guards, run full validation,
      publish the evidence matrix, and close the lane

## Owner And Seam

Live parsers, descriptors, config types, and behavior tests remain source-side
truth. Active docs own explanation and routing. This lane may adjust docs code
and tests that render or verify documentation; it may not change runtime
meaning to make documentation easier.

## Non-Goals

- no command, runtime, manifest, JSON, or container behavior change
- no release or CI workflow work
- no rewrite of archives or historical evidence
- no unrelated docs cleanup

## Acceptance

- [x] the evidence matrix covers the whole current public surface by behavior
      family, not only the August managed-runtime changes
- [x] all verified documentation gaps are repaired
- [x] the skill and built-in docs explain the recent headless, secret,
      readiness, workspace-ownership, and non-console exec behavior
- [x] recurrence checks cover relationships that are stable enough to enforce
- [x] required focused and full validation pass
- [x] one dated closeout log records findings, changes, checks, and residuals

## Evidence

- Planning: [`2026-08/21-224918-documentation-coverage-parity-planning.md`](../../logs/2026-08/21-224918-documentation-coverage-parity-planning.md)
- Closeout: [`2026-08/21-230738-documentation-coverage-parity-closeout.md`](../../logs/2026-08/21-230738-documentation-coverage-parity-closeout.md)

## Next Task

Lane complete. Review the worker PR; do not merge without operator
authorization. Run the second governance review by 2026-09-17 and await
operator intent for the next Horizon theme.
