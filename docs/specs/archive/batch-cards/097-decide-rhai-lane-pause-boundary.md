# 097 Decide Rhai Lane Pause Boundary

Status: complete
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/archive/004-rust-native-scripting-strict-lane.md`

## Objective

Decide whether the Rhai lane should now pause cleanly on the shipped Effigy
dogfooding boundary until an external pilot repo becomes safe again.

## In Scope

- assess whether Effigy's internal Rhai proof is broad enough for a pause
- record which remaining shell boundaries are intentional, not backlog drift
- decide whether any immediate Effigy-only capability gap still justifies one
  more internal batch
- if the honest answer is “pause”, open the lane-exit or watchpoint surface

## Out Of Scope

- reopening Keepsake while its repo boundary is unsafe
- touching Jetstream
- speculative new Rhai APIs without a concrete proving target

## Acceptance Criteria

- the lane has an explicit decision on pause vs one-more-internal-batch
- the decision is recorded against `g02.004`
- one clear next card exists after the decision

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Decision was “one last explicit internal card”, which led directly into native
distribution cutover work instead of a lane pause.
