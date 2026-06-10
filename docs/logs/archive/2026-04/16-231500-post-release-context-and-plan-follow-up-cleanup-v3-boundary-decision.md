# 215 Post Release Context And Plan Follow Up Cleanup V3 Boundary Decision

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `post-release-context-and-plan-follow-up-cleanup-v3-boundary-decision`

## Summary

Kept the release seam open after `214`.

The context-loading and plan-collection layer is now crate-owned, but
`src/runner/release_command.rs` still carries one more coherent release-domain
cluster around apply/gate execution and final execute result shaping.

## Decision

Do not pause the release seam yet.

The remaining runner file is smaller, but it still owns:

- `execute_release_prepare(...)`
- `execute_release(...)`
- `run_release_gates(...)`
- standalone gate-run shaping and release-progress adaptation around that flow

That is still more than honest prompt IO or final CLI dispatch. The next move
should be one more bounded release cleanup batch, not a seam switch.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release context and plan collection still inline in runner` -> current `context/plan collection is crate-owned, leaving apply/gate execution as the next remaining release-domain shell`
- Remaining gap: `src/runner/release_command.rs` still carries apply/gate
  execution and final execute result shaping before the release seam can pause

## Validation Performed

- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- the release shell is materially smaller, but pausing here would still freeze
  one runner-owned release-domain cluster before `v0.3`
- the docs pass still emits unrelated demo warnings from parallel work, but no
  release warning residue remains

## Next Task

- Execute `216-implement-effigy-release-apply-and-gate-follow-up-cleanup-v4.md`.
