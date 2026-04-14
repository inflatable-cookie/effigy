# 095 Decide Post-Release-Wrapper Rhai Slice

Status: ready
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`

## Objective

Decide the next bounded Rhai migration slice now that Effigy has broadened
dogfooding from tasks and demos into compatibility release-wrapper surfaces.

## In Scope

- assess whether the current Rhai host/runtime surface is broad enough to
  reopen the first external pilot
- decide whether the next slice should be:
  - reopening the first external pilot
  - one more Effigy-only dogfooding batch
  - or one bounded host-API refinement
- keep Keepsake deferred unless the repo boundary is explicitly safe again

## Out Of Scope

- implementing the next migration batch
- Jetstream work
- broad scripting-policy replanning

## Acceptance Criteria

- one explicit next bounded slice is chosen
- the lane keeps exactly one ready card
- the decision records whether Effigy dogfooding is now broad enough to reopen
  an external repo boundary

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

After this decision, execute the chosen next Rhai migration slice instead of
reopening broad scripting-boundary debate.
