# 667 - Close State Runner Thin Shell Proof

Roadmap: [`../035-state-domain-extraction.md`](../035-state-domain-extraction.md)
Strict lane: [`../../../specs/071-state-domain-extraction-strict-lane.md`](../../../specs/071-state-domain-extraction-strict-lane.md)
Contract: [`../../../contracts/027-state-domain-extraction-contract.md`](../../../contracts/027-state-domain-extraction-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Close `g04.035` after state report paths, history, apply planning, and capture
planning have moved into `effigy-state`.

## Scope

- confirm runner state code now owns orchestration, side effects, and rendering
- confirm `effigy-state` owns the extracted pure domain pieces
- record validation
- mark strict lane `071` complete
- mark roadmap `g04.035` complete
- update front doors to point to `g04.036`

## Non-Goals

- no further implementation work
- no manifest decomposition work in this card
- no media/object-store implementation
- no release execution

## Acceptance

- `g04.035` is complete
- strict lane `071` is complete
- contract `027` reflects the implemented boundary
- next task points to `g04.036`
- validation status is recorded

## Outcome

- closed `g04.035`
- closed strict lane `071`
- updated contract `027` with the implemented state-domain boundary
- selected `g04.036` manifest section decomposition as the next lane

## Validation

- `cargo test -p effigy-state` passed
- `cargo test state_apply` passed
- `cargo test state_capture` passed
- `cargo check --bin effigy` passed
- `git diff --check` passed

## Next Task

Open `g04.036` manifest section decomposition.
