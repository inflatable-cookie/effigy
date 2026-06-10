# 213 Post Release Text And Remediation Follow Up Boundary Decision

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `post-release-text-and-remediation-follow-up-boundary-decision`

## Summary

Kept the release seam open after `212`.

The text/remediation layer is now crate-owned, but
`src/runner/release_command.rs` still carries one more coherent release-domain
surface: release context loading plus status/prepare/simulate/execute-plan
collection. That is still more than honest terminal IO and final command
dispatch.

## Decision

- do not pause the release seam yet
- keep `g02.010` on release for one more bounded batch
- target the remaining context-loading and plan-collection layer in
  `src/runner/release_command.rs`

## Why This Boundary Is Not Honest Yet

The remaining release shell still includes:

- `ReleaseContext`
- `load_release_context(...)`
- `collect_release_status(...)`
- `collect_release_prepare_plan(...)`
- `collect_release_simulation(...)`
- `collect_release_execute_plan(...)`

That is still release-domain orchestration, not just interactive prompt IO and
runner dispatch glue.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release text/remediation extracted but context/plan collection still local` -> current `release pause rejected because context loading and plan collection are now the next clear seam`
- Remaining gap: `src/runner/release_command.rs` still owns release context and
  plan collection, plus the final interactive prompt and runner apply/dispatch shell

## Validation Performed

- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- the release shell is materially smaller, but pausing here would still freeze
  one obvious release-domain layer in the runner
- current warning residue is outside this seam in the parallel demo work, so it
  should not distract from the release boundary call

## Next Task

- Execute `214-implement-effigy-release-context-and-plan-follow-up-cleanup-v3.md`.
