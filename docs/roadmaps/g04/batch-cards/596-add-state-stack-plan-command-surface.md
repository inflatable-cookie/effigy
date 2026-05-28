# 596 - Add State Stack Plan Command Surface

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Expose the state-stack manifest foundation through a narrow planning command.

## Scope

- add the minimum CLI/parser path for a plan-only state-stack command
- load a manifest path and feed it into `effigy-state`
- render the lineage plan in text and JSON without executing hooks
- add focused command/parser tests
- keep the command clearly plan-only in help/output text

## Non-Goals

- no `apply`, `capture`, or `rebase` execution
- no app hook execution
- no durable lineage ledger
- no Example App-specific command behavior
- no release work

## Exit Condition

This card is complete when operators can run a plan-only state-stack command
against a manifest fixture and receive deterministic text/JSON output.

## Closeout

- added `effigy state plan <MANIFEST>`
- wired parser, help, repo targeting, JSON mode, command labels, and dispatch
- rendered deterministic text and JSON lineage output from `effigy-state`
- added parser and CLI output coverage for the plan-only surface
- added an `[Unreleased]` changelog entry

## Validation

- PASS: `cargo test -p effigy-state`
- PASS: `cargo test -p effigy --lib state_option_tests -- --nocapture`
- PASS: `cargo test -p effigy --lib parse_state_help_is_scoped -- --nocapture`
- PASS: `cargo test --test cli_output_tests state_command_tests -- --nocapture`

## Next Task

Card
[`597-close-state-stack-foundation-and-select-next-boundary.md`](./597-close-state-stack-foundation-and-select-next-boundary.md).
