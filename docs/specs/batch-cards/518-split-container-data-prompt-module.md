# 518 - Split Container Data Prompt Module

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move container data prompt policy and rendering out of the main data command
module.

## Scope

- create a focused prompt module under `container_command/data/`
- move data seed, import, pull-production, and destructive-action prompt
  policy helpers
- move prompt rendering and confirmation parsing helpers
- keep command orchestration in `container_command/data.rs`
- preserve current prompt text and non-interactive behavior

## Non-Goals

- no public CLI behavior changes
- no data feature work
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `container_command/data.rs` no longer owns prompt
policy/rendering logic.

## Closeout

Added `src/runner/container_command/data/prompts.rs` and moved prompt policy,
TTY gating, confirmation rendering, and confirmation parsing there. The main
data command file still coordinates runtime/container actions, but prompt
ownership is now separate and testable through the existing focused data tests.

## Validation

- `cargo test -p effigy --lib runner::container_command::data::tests -- --test-threads=1`
  passed
- `CARGO_TARGET_DIR=/tmp/effigy-g04-data-split-check cargo check -p effigy --lib`
  passed with the existing `runtime_activation_report_for_result` dead-code
  warning
- `git diff --check` passed

## Next Task

Start card
[`519-close-data-seed-dump-pipeline-and-open-rhai-lane.md`](./519-close-data-seed-dump-pipeline-and-open-rhai-lane.md).
