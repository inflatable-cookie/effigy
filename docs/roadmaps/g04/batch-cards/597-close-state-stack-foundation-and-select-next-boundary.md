# 597 - Close State Stack Foundation And Select Next Boundary

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Close the first state-stack foundation pass and choose the next boundary
deliberately.

## Scope

- review the shipped `effigy-state` crate and `effigy state plan` command
- decide whether the next card should add manifest integration, richer JSON
  contract examples, or the first apply/capture adapter
- update docs/contracts if the first implementation changed the planned
  boundary
- avoid adding execution behavior in this closeout card

## Non-Goals

- no `apply`, `capture`, or `rebase` execution
- no app hook execution
- no durable lineage ledger
- no release work

## Exit Condition

This card is complete when the foundation pass is closed and the next ready
card is selected from actual implementation evidence.

## Closeout

- reviewed `effigy-state` and `effigy state plan`
- kept the foundation boundary plan-only: manifest parsing, validation, text
  rendering, JSON lineage output, no app hooks
- clarified contract `016` so apply/capture reports stay future execution
  surfaces and the shipped command is explicitly lineage-plan only
- selected JSON contract examples as the next boundary before apply/capture
  execution

## Validation

- PASS: `cargo test -p effigy-state`
- PASS: `cargo test -p effigy --lib state -- --nocapture`
- PASS: `cargo test --test cli_output_tests state_command_tests -- --nocapture`
- PASS: docs path checks for changed planning docs
- PASS: `git diff --check`

## Next Task

Card
[`598-add-state-stack-json-contract-examples.md`](./598-add-state-stack-json-contract-examples.md).
