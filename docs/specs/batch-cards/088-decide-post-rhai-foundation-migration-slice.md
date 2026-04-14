# 088 Decide Post-Rhai Foundation Migration Slice

Status: complete
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`

## Objective

Choose the next bounded migration slice after the Rhai script-step foundation
landed in Effigy.

## In Scope

- assess what the Rhai foundation now honestly enables
- compare the next meaningful migration candidates
- decide which slice best proves the product boundary next
- update the lane and roadmap so one concrete next implementation card becomes
  authoritative

## Out Of Scope

- implementing the next migration slice
- broad multi-repo scripting conversion
- deep Jetstream engine-host capability design

## Acceptance Criteria

- the next Rhai migration slice is explicit
- the lane names one ready implementation card
- the repo order stays honest instead of expanding into simultaneous churn

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Decision:

- next slice is Effigy dogfooding
- Keepsake stays later
- Jetstream is explicitly deferred for now because active local work there
  makes it the wrong migration target this batch

Next card:

- [`089-implement-effigy-rhai-dogfooding-cluster.md`](./089-implement-effigy-rhai-dogfooding-cluster.md)
