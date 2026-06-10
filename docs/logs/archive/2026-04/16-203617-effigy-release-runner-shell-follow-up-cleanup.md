# 199 Effigy Release Runner Shell Follow Up Cleanup

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `effigy-release-runner-shell-follow-up-cleanup`

## Summary

Moved the remaining release review/prompt parsing and release text/status
rendering helpers out of `src/runner/release_command.rs` and into
`effigy-release`.

This batch made the runner keep terminal IO and irreversible release dispatch,
while the crate now owns:

- prepare/execute/resume menu parsing
- blocked-preflight action parsing
- indexed review inspection parsing
- release gate review line shaping
- reprepare and discard confirmation text shaping
- release-state discarded text rendering

## Changes

- widened `crates/effigy-release/src/review.rs` with crate-owned review menu
  parsing and shell-facing review line helpers
- widened `crates/effigy-release/src/text.rs` with release-state discarded text
  rendering
- re-exported those review/text contracts from
  `crates/effigy-release/src/lib.rs`
- rewired `src/runner/release_command.rs` onto the crate-owned review/text
  helpers
- removed the duplicate runner-owned review menu enums and helper block
- reduced `src/runner/release_command.rs` from `4549` lines to `2749`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release review/prompt parsing and shell-facing text helpers still partly runner-owned` -> current `review parsing and release text/status helper contracts are crate-owned, leaving a much narrower interactive runner shell`
- Remaining gap: `src/runner/release_command.rs` still carries interactive IO,
  final command dispatch, prompt confirmation flow, and runner-side adapter
  wiring

## Validation Performed

- command: `cargo fmt --all`
  - result: passed
- command: `cargo test release_command --lib`
  - result: passed after removing one duplicate local wrapper that shadowed the
    new crate export
- command: `cargo test --test cli_output_tests release`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- `release_command.rs` is still one of the larger remaining runner shells, so
  the next boundary decision needs to be strict about whether the remainder is
  now honest adapter work
- full `cargo test` is still not a reliable gate on this machine because the
  broader macOS toolchain path remains blocked by the unaccepted Xcode license

## Next Task

- Execute `201-decide-post-release-runner-shell-follow-up-cleanup-boundary.md`.
