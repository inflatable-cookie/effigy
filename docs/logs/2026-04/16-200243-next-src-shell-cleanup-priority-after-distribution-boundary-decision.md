# 2026-04-16 20:02:43 BST — Next Src Shell Cleanup Priority After Distribution Boundary Decision

## Summary

The next `/src` priority is `demo_command.rs`.

Distribution is now paused on an honest adapter shell. The next highest-value
cleanup target is [src/runner/demo_command/mod.rs](../../../src/runner/demo_command/mod.rs),
which is still the largest mixed-responsibility runner file in the root crate.

## Why This Decision

Current `/src` pressure after the distribution pause:
- `demo_command.rs`: `3302` lines
- `release_command.rs`: `2874` lines
- `container_command.rs`: `1276` lines
- `demo_browser.rs`: `1132` lines
- `docs_command.rs`: `1083` lines

Why demo goes first:
- it is the largest remaining runner shell
- it still mixes render/projection, command bridge flow, and raw runtime
  wiring in one file
- container is a bad choice for this thread right now because parallel work is
  actively changing the nearby container planning/crate area
- release is still large, but its remaining shell is more clearly interactive
  review/prompt flow, while demo still has a more extractable mixed shell

## Decision

Move next to bounded demo runner shell cleanup.

## Churn Check

This keeps the lane on broad `/src` cleanliness rather than getting stuck in
distribution after the domain seam is already clean enough to pause.

## Vision Target Delta

- primary vision tags: `MAINT`
- moved: the next shell-cleanup priority after distribution is now explicit
- remaining open: reduce the demo runner shell and then judge whether that seam
  can pause cleanly again

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`196-implement-effigy-demo-runner-shell-follow-up-cleanup.md`](../../../specs/batch-cards/196-implement-effigy-demo-runner-shell-follow-up-cleanup.md)
to reduce the next largest mixed-responsibility runner shell.
