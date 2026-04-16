# 174 Post Demo Managed Runtime And Backend Follow-up Boundary Decision

Created: 2026-04-16
Roadmap: g02.010
Batch: post-demo-managed-runtime-and-backend-follow-up-boundary-decision

## Summary
- Closed `174`.
- Paused the demo runner seam.
- Moved the lane to `release_command.rs` as the next `/src` pressure point.

## Changes
- recorded that the remaining `demo_command.rs` work is now mostly runner shell
  behavior
- classified `src/runner/release_command.rs` as the next meaningful
  modularization target
- opened `175` for one more `effigy-release` follow-up extraction batch
- updated currentness surfaces so the lane now points at `175`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `ROUTE`
- Movement: baseline `demo runner still carried reusable managed-runtime truth` -> current `demo runner now mostly reads as shell/orchestration, with release becoming the largest unresolved runner seam`
- Remaining gap: `src/runner/release_command.rs` still dominates the runner surface at `5581` lines and remains the clearest next candidate for another bounded crate extraction

## Validation Performed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- `release_command.rs` may still contain a mix of crate-worthy logic and true
  shell behavior, so `175` should stay tight

## Next Task
- Execute `175-implement-effigy-release-git-and-verify-install-follow-up-extraction.md`.
