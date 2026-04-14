# 090 Decide Post-Effigy Rhai Dogfooding Slice

Status: ready
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`

## Objective

Decide the next bounded Rhai scripting slice after the first substantial
Effigy-only dogfooding batch landed, using the real migration gaps from that
batch instead of guessing at cross-repo rollout.

## In Scope

- assess what the Effigy dogfooding batch proved
- decide whether the next slice should be:
  - more Effigy dogfooding
  - a bounded Rhai host-API expansion
  - or the first Keepsake pilot
- keep the lane honest about the lifecycle/signal gap exposed by
  `lifecycle-window`

## Out Of Scope

- implementing the next slice
- Jetstream migration planning
- broad cross-repo Rhai rollout

## Acceptance Criteria

- one clear next slice is chosen
- the decision explicitly handles the `lifecycle-window` signal/lifecycle gap
- the lane leaves exactly one new ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

After this decision lands, execute the chosen ready card immediately instead of
reopening broad scripting strategy debate.
