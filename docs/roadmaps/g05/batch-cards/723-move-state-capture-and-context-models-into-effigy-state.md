# 723 - Move State Capture And Context Models Into Effigy-State

Roadmap: [`../009-state-command-thin-shell-follow-through.md`](../009-state-command-thin-shell-follow-through.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)
Contract: [`../../../contracts/027-state-domain-extraction-contract.md`](../../../contracts/027-state-domain-extraction-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Land the first honest `state_command` shrink slice by moving stable capture and
context report models plus adjacent enum/string codec helpers into
`effigy-state`.

## Scope

- move stable capture report/context structs into `effigy-state`
- move capture artifact/task status enums and plain-string enum codec helpers
  where ownership is durable
- keep file writes, task execution, artifact staging, and hook execution in the
  runner
- keep current command behavior and payload shape stable

## Non-Goals

- no state CLI grammar changes
- no schema change
- no media/object-store work
- no manifest loading move in this card

## Acceptance

- the moved state models live in `effigy-state`
- `state_command.rs` shrinks and sheds stable type ownership
- current state command tests and `effigy-state` tests stay green

## Completed

- Moved stable state capture report/context structs into `effigy-state`.
- Moved capture artifact/task status enums and plain-string enum codec helpers
  into `effigy-state`.
- Rewired `state_command.rs` to construct and use the shared domain-owned types
  without changing the current state command behavior.

## Validation

- `cargo test -p effigy-state`
- targeted state command tests
- `effigy state plan --json`
- `effigy scan god-files --json`

## Stop Conditions

- stop if a moved type is already implicitly treated as a public schema that
  would change shape
- stop if the move requires runner side effects inside `effigy-state`

## Next Task

Execute `724` to continue the state thin-shell follow-through.
