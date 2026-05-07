# 045 - Runtime Activation Pipeline Strict Lane

Roadmap: [`g04.003`](../roadmaps/g04/003-runtime-activation-pipeline.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Move runtime prep and container activation into a typed pipeline.

This lane starts after `g04.002` moved the pure execution planning front half
into `effigy-execution`. The remaining size and ownership pressure in standard
and managed execution is mostly runtime/container behavior, so the next work
must create a narrow activation request, plan, and report surface before moving
side effects.

## Hard Boundaries

- no public CLI behavior changes unless a card explicitly selects a cleanup
  break
- no release work
- no `.github/workflows/` edits
- do not move backend-specific container command construction into runner
- keep side-effectful runtime prep in runner until pure activation plan types
  exist

## Current Ready Card

None. This lane is complete.

## Execution Chain

- `443` complete: close execution pipeline ownership and hand off runtime activation
- `444` complete: scaffold runtime activation pipeline lane
- `445` complete: scaffold effigy-runtime-plan crate
- `446` complete: select first runtime plan runner integration
- `447` complete: wire runtime activation plan into exec surface
- `448` complete: select next runtime activation integration
- `449` complete: wire runtime activation plan into DB seed
- `450` complete: select deferral or standard task runtime integration
- `451` complete: wire runtime activation plan into deferral
- `452` complete: wire runtime activation plan into standard task activation
- `453` complete: wire runtime activation plan into managed task activation
- `454` complete: select runtime prep stage migration slice
- `455` complete: move runtime prep activation executor behind plan
- `456` complete: extract runtime policy validation stage
- `457` complete: extract runtime running-state and ensure-running stages
- `458` complete: extract runtime mount preparation stage
- `459` complete: extract runtime compose up stage
- `460` complete: extract runtime exec readiness stage
- `461` complete: extract runtime alias reconciliation stage
- `462` complete: extract runtime gateway readiness stage
- `463` complete: extract runtime lease refresh stage
- `464` complete: decide runtime activation stage extraction closeout
- `465` complete: split runtime prep stage modules
- `466` complete: wire runtime activation plan into workspace sessions
- `467` complete: select next runtime activation caller migration
- `468` complete: route Rhai container exec through runtime activation
- `469` complete: decide runtime activation pipeline closeout

## Exit Condition

This lane closes when runtime activation has typed request, plan, and report
surfaces; standard, managed, exec, workspace, bootstrap, and Rhai container
paths consume the same activation pipeline; and no new caller-local activation
booleans are introduced.

## Next Task

Continue in
[`046-container-operation-pipeline-strict-lane.md`](./046-container-operation-pipeline-strict-lane.md).
