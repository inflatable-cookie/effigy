# 665 - Move State Plan Builders Into Effigy-State

Roadmap: [`../035-state-domain-extraction.md`](../035-state-domain-extraction.md)
Strict lane: [`../../../specs/071-state-domain-extraction-strict-lane.md`](../../../specs/071-state-domain-extraction-strict-lane.md)
Contract: [`../../../contracts/027-state-domain-extraction-contract.md`](../../../contracts/027-state-domain-extraction-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move pure state apply/capture plan-building behavior from runner-local structs
into `effigy-state`.

## Scope

- move apply report status planning from `StateStackApplyReport::from_lineage`
  into `effigy-state`
- move stable apply report layer/status types if they can remain side-effect
  agnostic
- move capture mode derivation and produced-layer planning if it can avoid
  runner-only artifact/task side effects
- keep hook execution, task execution, artifact staging, and SQL import in the
  runner
- preserve existing JSON report shape

## Non-Goals

- no hook execution changes
- no artifact staging/capture changes
- no SQL import changes
- no state command grammar changes
- no JSON schema changes
- no media/object-store implementation

## Caution

`state_command.rs` currently carries apply-hook changes. Do not move hook
execution into `effigy-state`. If apply report types move, they must still
support hook fields without changing the emitted payload.

## Acceptance

- pure apply planning is owned by `effigy-state`
- runner mutates planned apply reports only for side-effect results
- extracted types have focused `effigy-state` tests
- command output remains compatible
- `state_command.rs` gets smaller without hiding side effects in the domain
  crate

## Outcome

- moved apply report and apply layer report types into `effigy-state`
- moved apply layer status and hook status enums into `effigy-state`
- moved apply report construction from lineage into `effigy-state`
- runner now mutates apply reports only for task/artifact/sql/hook side-effect
  results
- preserved apply-hook execution behavior and payload shape

## Validation

- `cargo test -p effigy-state` passed
- `cargo test state_apply` passed
- `cargo check --bin effigy` passed
- `git diff --check` passed

## Next Task

Execute `666` to move remaining state history/read-model or capture-plan pieces
that are still runner-owned after `665`.
