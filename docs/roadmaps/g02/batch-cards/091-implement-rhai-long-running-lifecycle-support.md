# 091 Implement Rhai Long-Running Lifecycle Support

Status: archived
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/archive/004-rust-native-scripting-strict-lane.md`

## Objective

Add the bounded Rhai host/runtime support needed for signal-aware long-running
repo automation so Effigy can migrate `lifecycle-window` and similar
long-running shell glue without pretending Rhai should emulate a full shell.

## In Scope

- define a narrow long-running-script lifecycle contract for Rhai
- add only the host/runtime helpers needed for:
  - detecting stop/termination intent
  - writing final status/cleanup state on shutdown
  - looping without relying on shell traps
- migrate `lifecycle-window` if the new contract is sufficient
- record any remaining lifecycle gaps honestly

## Out Of Scope

- broad process supervision APIs inside Rhai
- arbitrary signal handling semantics beyond the bounded contract
- Keepsake migration
- Jetstream migration

## Acceptance Criteria

- Rhai can support one honest long-running first-party script lifecycle
- `lifecycle-window` either migrates cleanly or the exact remaining blocker is
  recorded
- the batch leaves the lane with one new explicit ready card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- demo run lifecycle-window`
- `cargo run --bin effigy -- demo stop lifecycle-window`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

After this batch, decide whether Effigy Rhai dogfooding is complete enough to
start the first Keepsake pilot or whether one more Effigy-only Rhai host-API
slice is still justified.
