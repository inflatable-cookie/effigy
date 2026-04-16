# 211 Post Release Runner Shell Follow Up Cleanup V2 Boundary Decision

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `post-release-runner-shell-follow-up-cleanup-v2-boundary-decision`

## Summary

Kept the release seam open after `210`.

The review menu/state/detail layer is now crate-owned, but
`src/runner/release_command.rs` still carries one more coherent release-domain
surface: the release text/projection and blocker-remediation layer. That is
already mirrored in `crates/effigy-release/src/text.rs`, so pausing now would
leave a real promoted seam half-adopted.

## Decision

- do not pause the release seam yet
- keep `g02.010` on release for one more bounded batch
- target `crates/effigy-release/src/text.rs` and the matching local text layer
  in `src/runner/release_command.rs`

## Why This Boundary Is Not Honest Yet

The remaining release shell is not just final prompt IO or command dispatch.
It still includes:

- `ReleaseBlockedStage`
- blocker remediation hint shaping
- release status text
- release prepare/simulate/prepared text
- release resume/execute plan text
- release verify-install and executed text

That is still release-domain projection logic, not final runner-shell glue.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release review layer extracted but text/remediation still local` -> current `release pause rejected because the remaining text/remediation layer is now the next clear seam`
- Remaining gap: `crates/effigy-release/src/text.rs` is still mostly dormant and `src/runner/release_command.rs` still owns the matching release text/projection helpers

## Validation Performed

- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- the docs pass is clean, but it still shows dead-code warnings from
  `crates/effigy-release/src/text.rs`, which is exactly the evidence that the
  seam should stay open
- pausing here would leave the release shell smaller, but not yet clean enough
  for the user’s `/src` cleanliness bar

## Next Task

- Execute `212-implement-effigy-release-text-and-remediation-follow-up-extraction.md`.
