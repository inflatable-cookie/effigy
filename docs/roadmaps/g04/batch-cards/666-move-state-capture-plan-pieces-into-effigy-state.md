# 666 - Move State Capture Plan Pieces Into Effigy-State

Roadmap: [`../035-state-domain-extraction.md`](../035-state-domain-extraction.md)
Strict lane: [`../../../specs/071-state-domain-extraction-strict-lane.md`](../../../specs/071-state-domain-extraction-strict-lane.md)
Contract: [`../../../contracts/027-state-domain-extraction-contract.md`](../../../contracts/027-state-domain-extraction-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move pure capture request planning pieces into `effigy-state` while keeping
capture task execution and artifact capture in the runner.

## Scope

- move capture mode derivation from role into `effigy-state`
- move produced capture layer construction into `effigy-state`
- move stable capture report structs if they can remain side-effect agnostic
- keep task execution, context file writes, artifact capture, and OCI publish in
  runner
- preserve existing capture JSON report shape

## Non-Goals

- no capture command grammar changes
- no artifact capture behavior changes
- no task hook execution changes
- no media/object-store implementation
- no provider/deploy behavior changes

## Acceptance

- pure capture planning is owned by `effigy-state`
- runner remains the owner of capture side effects
- extracted capture types have focused `effigy-state` tests
- `state capture` output remains compatible
- the remaining runner state code is primarily orchestration, side effects, and
  rendering

## Outcome

- moved capture mode derivation into `effigy-state`
- moved produced capture layer construction into `effigy-state`
- added a domain `StateCapturePlanRequest`
- kept task execution, context writes, artifact capture, and publish behavior in
  the runner
- preserved state capture payload compatibility

## Validation

- `cargo test -p effigy-state` passed
- `cargo test state_capture` passed
- `cargo check --bin effigy` passed
- `git diff --check` passed

## Next Task

Execute `667` to close the state runner thin-shell proof.
