# g08.023 - Dependency Link Portfolio Proof And Closeout

Status: Complete
Depends on: `g08.022`

## Goal

Prove the dependency-link contract against real portfolio shapes, publish the
operator guidance, and close the `g08.018` suite with reproducible evidence.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Target envelope: a maintainer can link, edit, rebuild, inspect, and unlink a
  local library across representative consumers without committed-source drift.
- Vision target delta: the dependency-link surface graduates from fixtures to
  cross-repo proof and documented everyday operation.

## Scope

- use temporary clean clones/worktrees so dirty live portfolio repos are not
  modified
- prove Signal `v0.1.0` against:
  - Soundcheck as the flat consumer shape
  - Loophole as the nested-workspace consumer shape
- prove full Signal closure selection, not only direct manifest entries
- prove a local Signal edit changes the consumer build input
- prove unlink returns Cargo resolution to the tag and returns lock state clean
- prove Bun save-less linking with synthetic published-package equivalents
  until the first real portfolio TS package is published
- prove Bun link drift after install and idempotent repair
- publish the local dependency linking guide and update command/help matrices
- update agent guidance for driving text/JSON status safely
- close all roadmap/front-door/currentness surfaces with validation evidence

## Non-Goals

- no portfolio manifest migration in this Effigy milestone
- no mutation of dirty live Signal/Soundcheck worktrees
- no claim of real published TS portfolio acceptance before publication exists
- no release execution

## Execution Plan

- [x] [`1062`](./batch-cards/1062-prove-signal-links-across-flat-and-nested-consumers.md)
      — prove Signal against disposable Soundcheck and Loophole clones
- [x] [`1063`](./batch-cards/1063-prove-bun-closure-drift-and-repair.md)
      — prove real save-less Bun closure, install drift, peer evidence, and
      repair against isolated portfolio-shaped fixtures
- [x] [`1064`](./batch-cards/1064-publish-dependency-link-guidance-and-close-suite.md)
      — publish operator/agent guidance, consolidate proof, and close the suite

## Acceptance Criteria

- [x] Soundcheck proof shows all matching Signal crates resolve locally, a
      local edit is consumed, and unlink restores tagged resolution/clean lock
- [x] Loophole proof covers every nested Cargo workspace that consumes Signal
- [x] Bun proof shows full closure, zero manifest/lock churn, install drift, and
      re-link repair
- [x] docs explain desired state, physical mechanisms, lock hygiene, peer
      dedupe, dry-run, status, and recovery
- [x] command reference and JSON examples are current
- [x] `effigy qa` passes, except any independently documented upstream release
      blocker that does not affect this feature
- [x] suite and front doors close with one explicit next task

## Evidence

- [`Signal Cargo portfolio proof`](../../logs/2026-08/05-225229-signal-cargo-portfolio-proof.md)
- [`Bun closure drift and repair proof`](../../logs/2026-08/05-230446-bun-closure-drift-repair-proof.md)
- [`Dependency linking suite closeout`](../../logs/2026-08/05-231121-dependency-linking-suite-closeout.md)
- [`Operator guide`](../../guides/077-local-dependency-linking.md)

## Validation

- temporary-repo cross-repo proof scripts/fixtures
- targeted Cargo/Bun manager integration tests
- command/help/JSON contract checks
- docs checks
- `effigy qa`

## Next Task

Select the next substantial g08 scope separately. No release or generation
rollover is implied.
