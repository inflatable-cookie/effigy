# 201 Post Release Runner Shell Follow Up Cleanup Boundary Decision

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `post-release-runner-shell-follow-up-cleanup-boundary-decision`

## Summary

Kept the release seam open.

`199` removed the review/prompt parsing and shell-facing release text helper
layer from `src/runner/release_command.rs`, but the file is still not down to
honest adapter-only work. The remaining shell still owns one more coherent
release-domain cluster around context loading, plan collection, and execute
orchestration helpers.

## Decision

Do not pause the release seam yet.

The remaining `src/runner/release_command.rs` weight is still shaped by:

- release context loading
- prepare/status/simulate plan collection
- execute orchestration helpers and progress mapping
- release-specific runner error/adaptation wiring

That is narrower than before, but it is still release-domain API, not just
terminal IO and final shell dispatch.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release runner shell still needed a strict post-cleanup classification` -> current `the review/text helper layer is accepted as shipped, and the remaining release seam is now isolated to one more bounded context/execute cleanup target`
- Remaining gap: `src/runner/release_command.rs` still carries a release-domain
  context/plan/execute shell cluster and cannot pause honestly yet

## Validation Performed

- command: `cargo test`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- release remains the largest unresolved runner seam, so one more extraction
  batch should still be kept meaningful rather than fragmented
- parallel container-design work is active in other crates and docs, so the
  lane should stay anchored on release rather than drifting sideways

## Next Task

- Execute `202-implement-effigy-release-context-and-execute-shell-follow-up-cleanup.md`.
