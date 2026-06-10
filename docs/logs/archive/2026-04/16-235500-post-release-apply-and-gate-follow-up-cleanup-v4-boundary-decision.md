# 217 Post Release Apply And Gate Follow Up Cleanup V4 Boundary Decision

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `post-release-apply-and-gate-follow-up-cleanup-v4-boundary-decision`

## Summary

Paused the release seam after `216`.

The remaining `src/runner/release_command.rs` surface is now mostly
interactive runner-shell work rather than unreduced release-domain logic.

## Decision

Pause the release seam here.

After `216`, the remaining runner file is primarily:

- interactive prepare / execute / resume review loops
- prompt and section-browser IO
- local version-override parsing and interactive menu handling
- final CLI dispatch and runner error mapping
- progress-line emission around crate-owned release APIs

That is an honest runner-shell boundary. It is no longer the next best
`effigy-release` extraction target.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release apply/gate execution still inline in runner` -> current `release seam paused on an honest interactive runner-shell boundary`
- Remaining gap: `None` inside the release-domain crate boundary; broader `/src`
  shell cleanup still remains elsewhere

## Validation Performed

- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- release is no longer the active seam, but broader root-crate shell cleanup is
  still open across demo, docs, contracts, and UI surfaces
- the docs pass still emits unrelated demo warnings from parallel work

## Next Task

- Execute `218-decide-next-src-shell-cleanup-priority-after-release-final-pause-boundary.md`.
