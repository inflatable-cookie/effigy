# 177 Effigy Release Git Execute Follow-up Extraction

Created: 2026-04-16
Roadmap: g02.010
Batch: effigy-release-git-execute-follow-up-extraction

## Summary
- Closed `177`.
- Moved the git-facing execute cluster into `effigy-release`.
- Left the lane on a release boundary decision instead of guessing another
  release slice.

## Changes
- added crate-owned git execute helpers in `crates/effigy-release/src/lib.rs`
- moved branch/head/remote inspection, local tag checks, working-tree status,
  staging, commit creation, tag creation, and push orchestration into
  `effigy-release`
- rewired `src/runner/release_command.rs` to adapt those crate-owned helpers
- removed the duplicate runner-owned git helper block
- reduced `src/runner/release_command.rs` from `5346` lines to `5098`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release execute still owns git-facing execute helpers in runner` -> current `git-facing execute and git-state inspection are crate-owned, runner now keeps the review shell and final runtime adapter behavior`
- Remaining gap: `src/runner/release_command.rs` still carries interactive review flow, final progress/render wiring, and remaining shell-facing execute orchestration

## Validation Performed
- command: `cargo fmt --all`
  - result: passed
- command: `cargo test -p effigy-release`
  - result: passed
- command: `cargo test release_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests release`
  - result: passed after one rerun; the first run hit an existing temp-fixture collision while creating a bare git remote
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- `release_command.rs` is still the largest unresolved runner seam
- the next decision needs to be strict about whether what remains is truly
  shell work or still one more crate-worthy release layer

## Next Task
- Execute `178-decide-post-release-git-execute-follow-up-boundary.md`.
