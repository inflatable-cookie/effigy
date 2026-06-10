# 210 Effigy Release Runner Shell Follow Up Cleanup V2

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `effigy-release-runner-shell-follow-up-cleanup-v2`

## Summary

Moved the release review menu/state/detail shell out of
`src/runner/release_command.rs` and into `effigy-release`.

This batch kept the final interactive runner loop and release text projection
local, while the crate now owns the duplicated review-layer contracts that
were still sitting inline in the runner.

## Changes

- widened `crates/effigy-release/src/lib.rs` to wire and export the promoted
  release review layer
- moved the release review enums, menu parsers, indexed-review helpers, and
  review render helpers behind crate-owned APIs
- rewired `src/runner/release_command.rs` onto those crate-owned review APIs
- kept the broader release text/render layer and final interactive runner loop
  local so the next move stays a real boundary decision instead of fake scope
  creep
- reduced `src/runner/release_command.rs` from `4549` lines to `3842`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release review menu/state/detail shell still duplicated inline in runner` -> current `release review-layer contracts live in effigy-release, leaving a narrower runner shell`
- Remaining gap: `src/runner/release_command.rs` still carries release text and
  projection helpers plus the final interactive prompt/review loop and runner
  error/progress wiring

## Validation Performed

- command: `cargo fmt --all`
  - result: passed
- command: `cargo test release_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests release`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- `crates/effigy-release/src/text.rs` is now more visible as the next possible
  release seam, but it is not yet wired broadly enough to claim as shipped in
  this batch
- `release_command.rs` is materially smaller, but still large enough that the
  next boundary decision must stay strict about whether the remaining text and
  interactive shell is honest adapter work

## Next Task

- Execute `211-decide-post-release-runner-shell-follow-up-cleanup-v2-boundary.md`.
