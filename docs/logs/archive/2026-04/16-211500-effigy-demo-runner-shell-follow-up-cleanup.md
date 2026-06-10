# 2026-04-16 21:15:00 BST — Effigy Demo Runner Shell Follow Up Cleanup

## Summary

Widened `effigy-demo` again so the reusable demo runner display/projection layer
no longer sits inline in `src/runner/demo_command.rs`.

The crate now owns:
- demo query shaping
- demo display field projection
- demo table projection
- active-attempt and active-terminal-session display shaping
- entrypoint construction and gap classification helpers

`src/runner/demo_command.rs` now adapts those crate-owned projections into
runner render types instead of building them inline.

## Why This Batch

After the distribution pause, the next biggest remaining `/src` seam was still
the demo runner shell. The most honest bounded slice was not the raw runtime
loop again, but the record/display/projection layer that still mixed domain
shaping with runner rendering.

## What Changed

- widened `crates/effigy-demo/src/records.rs`
- added crate-owned display-field and table-projection contracts
- moved query, action, history-table, active-attempt, and active-session
  projection shaping into `effigy-demo`
- moved demo entrypoint construction and gap classification helpers into the
  crate-owned demo surface
- rewired `src/runner/demo_command.rs` to adapt those projections into
  `KeyValue` and `TableSpec`

## Churn Check

This was still a meaningful shell-cleanup batch, not atomized tidy-up.
`src/runner/demo_command.rs` dropped from `3302` lines to `2959`, and the
remaining weight is now more clearly runner-side command/process orchestration.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: demo runner projection and display shaping from `runner` into
  `effigy-demo`
- remaining open: decide whether the remaining demo runner shell is now honest
  enough to pause or still needs one more bounded cleanup slice

## Validation

- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`
- `cargo test --test cli_output_tests demo`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

Full `cargo test` was attempted but is currently blocked on this machine by the
unaccepted Xcode license, which causes macOS linker failures outside the demo
slice.

## Next Task

Execute
[`197-decide-post-demo-runner-shell-follow-up-cleanup-boundary.md`](../../../specs/batch-cards/197-decide-post-demo-runner-shell-follow-up-cleanup-boundary.md)
to decide whether the remaining demo runner shell can now pause cleanly.
