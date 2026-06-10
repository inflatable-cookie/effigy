# 205 Post Release Interactive And Apply Shell Follow Up Cleanup Boundary Decision

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `post-release-interactive-and-apply-shell-follow-up-cleanup-boundary-decision`

## Summary

Paused the release seam.

`204` removed the release apply/orchestration layer from
`src/runner/release_command.rs`, and the remaining file is now shaped mostly by
interactive runner-shell work rather than reusable release-domain API.

## Decision

Pause the release seam on an honest shell boundary.

The remaining `src/runner/release_command.rs` weight is now mostly:

- interactive prepare/execute/resume review loops
- prompt and section-browser IO
- version override validation and prompt-local parsing
- runner-side command dispatch and error mapping
- final progress emission wrapper behavior

That is still real code, but it is now runner-shell orchestration rather than
the next obvious `effigy-release` extraction target. Keeping the release seam
open longer would be fake completeness work.

`g02.010` does not pause, though. `/src` still has larger shell-heavy seams,
so the next move is a fresh `/src` priority decision rather than treating
release pause as lane completion.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release shell still needed a strict post-cleanup classification` -> current `release is now paused on an honest interactive runner-shell boundary`
- Remaining gap: `/src` still has larger shell-heavy seams outside release and
  the lane remains active

## Validation Performed

- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- demo remains the largest runner shell, but it was already paused on a judged
  shell boundary, so the next priority decision still needs a churn check
  instead of automatic reopening
- parallel container work is active in another thread, so the next shell
  priority should avoid unnecessary write-set overlap

## Next Task

- Execute `206-decide-next-src-shell-cleanup-priority-after-release-pause-boundary.md`.
