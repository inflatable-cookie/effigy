# 182 Post Changelog Workspace Extraction Boundary Decision

Created: 2026-04-16
Roadmap: g02.010
Batch: post-changelog-workspace-extraction-boundary-decision

## Summary
- Closed `182`.
- Kept the release seam open.
- Opened `183` for one last bounded release extraction batch.

## Changes
- recorded that `src/runner/release_command.rs` is still not honest shell work
  after the changelog workspace move
- classified the remaining reusable layer as the interactive release review and
  text-projection cluster
- updated currentness surfaces so the lane now points at `183`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `changelog coupling is still the last obvious release
  library seam` -> current `changelog is workspace-owned, but the release
  review/menu/text-projection layer is still the next reusable cluster`
- Remaining gap: `src/runner/release_command.rs` still owns interactive review
  state, review menus, blocked-preflight/drift review shaping, and a large
  release-specific text projection shell

## Validation Performed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- this needs to stay one final bounded release batch, not turn into another
  whole-file rewrite
- if `183` still leaves a large mixed shell, `g02.010` should stay open rather
  than pausing optimistically

## Next Task
- Execute `183-implement-effigy-release-review-and-text-projection-follow-up-extraction.md`.
