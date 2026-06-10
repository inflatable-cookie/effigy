# Init Setup Wizard Lane Opened

Date: 2026-05-19  
Roadmap: [`g07.050`](../../../roadmaps/g07/050-init-setup-wizard-suite.md)  
Batch card: [`1000`](../../../roadmaps/g07/batch-cards/1000-open-init-setup-wizard-lane.md)  
Strict lane: [`093`](../../../specs/093-init-setup-wizard-strict-lane.md)

## What Changed

- opened the init setup-wizard suite under `g07`
- opened strict lane `093`
- added the ordered roadmap set for:
  - checklist contract
  - TTY wizard flow
  - setup-job adapters and safety bounds
  - non-interactive action execution
  - proof and closeout
- moved front-door planning surfaces so `continue` resolves to `1001`

## Scope Decision

This lane keeps `effigy init` as the only onboarding front door.

The owned work is:

- one shared setup-job inventory
- TTY-only prompt orchestration for plain `effigy init`
- `--checklist --json` for agent planning
- explicit non-interactive execution for selected setup actions

The lane explicitly excludes:

- a second onboarding command
- release/deploy/state/distribution mutation through init
- a generic interactive framework widened beyond init

## Validation

- `./target/debug/effigy docs check links ...`
- `./target/debug/effigy docs check paths ...`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: init now has an active execution lane for setup orchestration rather
  than only a narrow baseline-file contract
- remains open: checklist contract, TTY wizard behavior, adapter wiring,
  explicit non-interactive execution, and proof/docs closeout

## Next Task

Execute `1001`.
