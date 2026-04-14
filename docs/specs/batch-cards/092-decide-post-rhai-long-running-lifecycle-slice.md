# 092 Decide Post-Rhai Long-Running Lifecycle Slice

Status: ready
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`

## Objective

Decide the next bounded Rhai migration slice now that Effigy has one honest
long-running stop-aware demo running under the Rhai surface.

## In Scope

- evaluate whether Effigy dogfooding has exposed enough of the next real host
  API gaps
- decide whether the next slice should be:
  - another Effigy-only Rhai dogfooding batch
  - a Keepsake pilot
  - or one more bounded scripting-surface contract refinement
- keep Jetstream explicitly out of scope while active local work continues

## Out Of Scope

- implementing the next migration batch
- cross-repo Jetstream work
- broad scripting-policy replanning

## Acceptance Criteria

- one explicit next bounded slice is chosen
- the lane keeps exactly one ready card
- the decision records whether Effigy dogfooding is now sufficient to widen
  into another repo

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

After this decision, execute the chosen next Rhai migration slice instead of
reopening broad scripting-boundary debate.
