# 690 - Close Reference Grade Cleanup Suite

Roadmap: [`../039-artifact-and-crate-boundary-rejustification.md`](../039-artifact-and-crate-boundary-rejustification.md)
Strict lane: [`../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md`](../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md)
Contract: [`../../../contracts/031-artifact-and-crate-boundary-contract.md`](../../../contracts/031-artifact-and-crate-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Close `g04.039` and the post-v0.6.x cleanup suite.

## Acceptance

- focused tests pass
- god-file scan is reviewed
- g04 README points to the next generation decision

## Outcome

- closed `g04.039`
- closed the post-v0.6.x reference-grade cleanup suite
- confirmed `effigy-artifacts` no longer appears in god-file scan output
- recorded remaining god-file warnings for future work:
  `src/runner/state_command.rs` and `crates/effigy-release/src/lib.rs`

## Validation

- `cargo test -p effigy-artifacts`
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `effigy scan god-files --json`
- `git diff --check`

## Next Task

Close or roll over g04 after this suite.
