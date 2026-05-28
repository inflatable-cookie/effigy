# 663 - Classify State Command Domain And Adapter Responsibilities

Roadmap: [`../035-state-domain-extraction.md`](../035-state-domain-extraction.md)
Strict lane: [`../../../specs/071-state-domain-extraction-strict-lane.md`](../../../specs/071-state-domain-extraction-strict-lane.md)
Contract: [`../../../contracts/027-state-domain-extraction-contract.md`](../../../contracts/027-state-domain-extraction-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Map `state_command.rs` responsibilities into domain, adapter, rendering, and
side-effect groups before moving code.

## Scope

- inspect the current `state_command.rs` diff and preserve unrelated edits
- list state report structs currently owned by runner
- list state path/history helpers currently owned by runner
- list pure plan builders and validation helpers currently owned by runner
- list side-effect adapters that must remain runner-owned for now
- decide the safest first extraction target for `664`

## Non-Goals

- no implementation extraction in this card
- no command grammar changes
- no JSON schema changes
- no media/object-store behavior
- no Example App-specific behavior

## Acceptance

- current state-command responsibilities are classified
- existing worktree edits are acknowledged and protected
- `664` has a precise first extraction target
- contract `027` is updated if evidence changes the planned boundary

## Outcome

- classified current state-command ownership in contract `027`
- confirmed `effigy-state` already owns manifest validation and lineage planning
- identified report path/history helpers as the safest first extraction target
- recorded the unrelated apply-hook worktree edits and protected them from the
  first extraction slice

## Suggested Evidence Commands

```sh
git diff -- src/runner/state_command.rs
rg -n "struct .*State|enum .*State|fn .*state|history|report|path|plan|capture|apply" src/runner/state_command.rs crates/effigy-state/src
wc -l src/runner/state_command.rs crates/effigy-state/src/lib.rs
```

## Validation

- docs review
- `git diff --check`

## Next Task

Execute `664` by moving state report path and history helpers into
`effigy-state`.
