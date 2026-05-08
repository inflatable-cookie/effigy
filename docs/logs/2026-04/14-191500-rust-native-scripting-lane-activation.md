# Rust-Native Scripting Lane Activation

Date: 2026-04-14

## Outcome

Activated a new strict planning lane for Rust-native scripting after the
released demo/browser lane closed.

## Why

- demo/browser delivery is shipped and no longer the active product question
- manifest cleanup across consumer repos exposed scripting sprawl as the next
  cross-repo consolidation problem
- the operator direction is now explicit:
  - Rust-first repos should avoid depending on Bun where possible
  - Rhai is the leading candidate for Effigy-native scripting
  - Jetstream should be treated as a full migration target, not a permanent
    Python exception

## New Governing Surfaces

- `docs/roadmaps/g02/004-rust-native-scripting-surface-contract.md`
- `docs/specs/004-rust-native-scripting-strict-lane.md`
- `docs/roadmaps/g02/batch-cards/086-decide-rust-native-scripting-boundary-and-pilot-slice.md`

## Next Task

Execute `086-decide-rust-native-scripting-boundary-and-pilot-slice.md`.
