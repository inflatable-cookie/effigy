# 203 Post Release Context And Execute Shell Follow Up Cleanup Boundary Decision

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `post-release-context-and-execute-shell-follow-up-cleanup-boundary-decision`

## Summary

Kept the release seam open.

`202` removed the release context and plan-collection layer from
`src/runner/release_command.rs`, but the file is still not down to honest
adapter-only work. The remaining shell still owns one more coherent cluster
around interactive review loops, prepare/apply flow, execute/apply flow, and
release-specific progress/error adaptation.

## Decision

Do not pause the release seam yet.

The remaining `src/runner/release_command.rs` weight is now shaped by:

- interactive prepare/execute/resume review loops
- prompt and section-browser IO
- release prepare apply flow
- release execute apply flow
- release gate progress/error adaptation

That is narrower than before, but it is still more than final terminal IO
glue. One more bounded release follow-up batch is still justified before this
seam can pause honestly.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release context/plan shell still needed strict post-cleanup classification` -> current `the context and execute-plan collection layer is accepted as shipped, and the remaining release seam is now isolated to one final interactive/apply shell cleanup target`
- Remaining gap: `src/runner/release_command.rs` still carries interactive
  review/apply shell behavior and cannot pause honestly yet

## Validation Performed

- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- release remains one of the larger unresolved runner shells, so the next batch
  should stay meaningful instead of fragmenting into helper-level churn
- parallel container-design work is still active in other crates and docs, so
  this lane should keep its focus on release or another established shell seam

## Next Task

- Execute `204-implement-effigy-release-interactive-and-apply-shell-follow-up-cleanup.md`.
