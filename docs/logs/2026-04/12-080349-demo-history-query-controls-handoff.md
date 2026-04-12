---
title: Demo history query controls handoff
status: active
owner: Platform
updated: 2026-04-12
tags: [coordination, handoff]
---

## What This Thread Was Doing

This thread has been driving the active `g02.003` demo-harness lane forward in
bounded batches. It shipped the first demo browser, then intentionally stopped
widening browser density and moved back down into runner-owned history
semantics. The most recent execution batch implemented
`effigy demo history <DEMO_ID> --attempt <ATTEMPT_ID>` so one retained
historical result can be inspected cleanly in text and JSON. The next planning
batch then closed on the conclusion that the next honest gap is still
query-first: one-demo history narrowing and more human-friendly selection
ergonomics, not more browser churn or list density.

## Why It Matters

The larger product goal in `g02.003` is to make demos a first-class
verification surface inside Effigy rather than a pile of repo-local scripts.
History review is part of that product boundary. If one-demo result review
stays weak, later browser or desktop work will end up inventing semantics
through presentation instead of consuming a settled runner contract.

## Current State

- Done so far: registry loading, inspect, run/stop/rerun, browser baseline,
  retained attempt history, dedicated `demo history`, and historical-attempt
  drilldown are all shipped and pushed on `main`.
- Still open: `039` needs to implement bounded history-query narrowing and a
  more human-friendly retained-attempt selection path on top of the existing
  stable `--attempt <ATTEMPT_ID>` contract.
- Active spec lane: [003-demo-harness-model-and-runner-strict-lane.md](/Users/betterthanclay/Dev/projects/effigy/docs/specs/003-demo-harness-model-and-runner-strict-lane.md)
- Canonical refs:
  - [003-demo-harness-model-and-runner-contract.md](/Users/betterthanclay/Dev/projects/effigy/docs/roadmaps/g02/003-demo-harness-model-and-runner-contract.md)
  - [039-implement-demo-history-query-controls.md](/Users/betterthanclay/Dev/projects/effigy/docs/specs/batch-cards/039-implement-demo-history-query-controls.md)
  - [11-demo-post-history-drilldown-boundary-decision.md](/Users/betterthanclay/Dev/projects/effigy/docs/logs/2026-04/12-demo-post-history-drilldown-boundary-decision.md)
- Remaining continuation envelope: one bounded runner-side batch, `039`, is in-bounds and ready.
- Lane budget / pause signal: the run stopped because the harness/thread broke and a fresh-thread continuation artifact was explicitly requested, not because the lane is blocked.
- Key files:
  - [demo_command.rs](/Users/betterthanclay/Dev/projects/effigy/src/runner/demo_command.rs)
  - [command_parsing.rs](/Users/betterthanclay/Dev/projects/effigy/src/cli/parse/command_parsing.rs)
  - [demo.rs](/Users/betterthanclay/Dev/projects/effigy/src/cli_help/topics/demo.rs)
  - [command_behavior_tests.rs](/Users/betterthanclay/Dev/projects/effigy/tests/cli_output_tests/command_behavior_tests.rs)
  - [039-implement-demo-history-query-controls.md](/Users/betterthanclay/Dev/projects/effigy/docs/specs/batch-cards/039-implement-demo-history-query-controls.md)
  - [11-demo-history-attempt-drilldown-implementation.md](/Users/betterthanclay/Dev/projects/effigy/docs/logs/2026-04/12-demo-history-attempt-drilldown-implementation.md)

## Boundaries

- Stay within `g02.003` runner/query history work under [003-demo-harness-model-and-runner-contract.md](/Users/betterthanclay/Dev/projects/effigy/docs/roadmaps/g02/003-demo-harness-model-and-runner-contract.md).
- Do not reopen browser density work, `demo list` history density, multi-demo aggregation, generic analytics, or broader runtime cancellation.
- Follow repo constraints from [AGENTS.md](/Users/betterthanclay/Dev/projects/effigy/AGENTS.md).

## Important Context

- Planning lineage: `037` shipped historical-attempt drilldown in commit `aab06d1`; `038` then chose bounded history-query controls as the next slice in commit `3778048`. The active ready card is now `039`.
- Spec-to-canonical relationship: the strict spec lane is only the execution grammar. The roadmap contract in [003-demo-harness-model-and-runner-contract.md](/Users/betterthanclay/Dev/projects/effigy/docs/roadmaps/g02/003-demo-harness-model-and-runner-contract.md) is the canonical promoted surface that the next thread should trust when implementing `039`.
- Decisions and preferences:
  - keep work in meaningful batches, not tiny micro-steps
  - always leave one explicit `Next Task`
  - do not use `--repo` when already in this repo
  - prefer `cargo run --bin effigy -- ...` for self-hosted validation because the installed released binary can lag the repo’s demo surface
  - user pushed hard against browser churn already; runner/query depth is preferred over more TUI tweaking right now
- Open tensions:
  - `039` should add genuinely useful narrowing/selection ergonomics without becoming generic timeline tooling
  - a likely shape is outcome-focused narrowing plus a human-friendly selection path such as ordinal/index selection, but that still needs one tight implementation judgment
  - do not widen the contract so far that the next batch becomes pseudo-analytics or multi-demo history

## Suggested Next Move

Start from [039-implement-demo-history-query-controls.md](/Users/betterthanclay/Dev/projects/effigy/docs/specs/batch-cards/039-implement-demo-history-query-controls.md), inspect the current `demo history` CLI and retained-attempt structures, then implement one bounded query-control batch in [demo_command.rs](/Users/betterthanclay/Dev/projects/effigy/src/runner/demo_command.rs), [command_parsing.rs](/Users/betterthanclay/Dev/projects/effigy/src/cli/parse/command_parsing.rs), [demo.rs](/Users/betterthanclay/Dev/projects/effigy/src/cli_help/topics/demo.rs), and the matching CLI tests. Keep it one-demo and query-first. A good target is: outcome-focused filtering plus one human-friendly retained-attempt selector alongside the existing stable `--attempt <ATTEMPT_ID>` path.

## Completion Protocol

1. Confirm [039-implement-demo-history-query-controls.md](/Users/betterthanclay/Dev/projects/effigy/docs/specs/batch-cards/039-implement-demo-history-query-controls.md) still reflects the exact bounded batch you are about to run.
2. Implement `039`, then update [003-demo-harness-model-and-runner-contract.md](/Users/betterthanclay/Dev/projects/effigy/docs/roadmaps/g02/003-demo-harness-model-and-runner-contract.md), the active currentness surfaces, and `docs/logs/README.md` in the same closeout.
3. Validate with the repo’s normal bar for this batch:
   - `cargo test`
   - `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
   - `cargo run --bin effigy -- qa`
   - `git diff --check`
4. Commit and push only after the lane state is current and the worktree is clean.
5. Leave one new explicit ready card and `Next Task` instead of free-continuing into browser or list work.
6. If `039` starts to depend on browser rendering, multi-demo aggregation, or generic analytics to feel coherent, stop and re-bound the lane instead of improvising implementation.
7. The immediate next task is: execute [039-implement-demo-history-query-controls.md](/Users/betterthanclay/Dev/projects/effigy/docs/specs/batch-cards/039-implement-demo-history-query-controls.md) as one substantial runner-side batch.
