# 173 Effigy Demo Managed Runtime And Backend Follow-up Extraction

Created: 2026-04-16
Roadmap: g02.010
Batch: effigy-demo-managed-runtime-and-backend-follow-up-extraction

## Summary
- Closed `173`.
- Moved the shared concurrent-runner runtime state and backend/projection truth
  into `effigy-demo`.
- Left the lane on a post-batch decision card instead of guessing one more
  extraction slice.

## Changes
- added shared concurrent-runner runtime state to
  `crates/effigy-demo/src/runtime.rs`
- moved non-zero-exit rendering there
- moved backend/projection shaping and browser-live-attach/input-target truth
  there
- rewired `src/runner/demo_command.rs` to adapt the extracted runtime layer
  while keeping raw supervisor/process orchestration local
- reduced `src/runner/demo_command.rs` from `3526` lines to `3302`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `ROUTE`
- Movement: baseline `demo runner still mixed between crate-owned runtime truth and runner shell behavior` -> current `shared managed-runtime/backend truth is crate-owned, runner now centers more clearly on orchestration shell work`
- Remaining gap: `src/runner/demo_command.rs` still carries raw process launch, supervisor orchestration, and final runner adapter behavior

## Validation Performed
- command: `cargo fmt --all`
  - result: passed
- command: `cargo test -p effigy-demo`
  - result: passed
- command: `cargo test demo_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests demo`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- `demo_command.rs` is still large enough that one more demo-specific batch may
  be justified
- if the next decision opens another demo card, the lane should do a churn
  check before continuing too many more atomic runner reductions

## Next Task
- Execute `174-decide-post-demo-managed-runtime-and-backend-follow-up-boundary.md`.
