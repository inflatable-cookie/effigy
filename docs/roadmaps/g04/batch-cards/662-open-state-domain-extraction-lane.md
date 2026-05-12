# 662 - Open State Domain Extraction Lane

Roadmap: [`../035-state-domain-extraction.md`](../035-state-domain-extraction.md)
Strict lane: [`../../../specs/071-state-domain-extraction-strict-lane.md`](../../../specs/071-state-domain-extraction-strict-lane.md)
Contract: [`../../../contracts/027-state-domain-extraction-contract.md`](../../../contracts/027-state-domain-extraction-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move `g04.035` from queued roadmap state into an active strict lane with one
bounded classification-first execution chain.

## Scope

- mark `g04.035` active
- add the strict lane
- add the contract anchor
- update the specs, contracts, and roadmap front doors
- select the first classification card
- record the existing `state_command.rs` worktree boundary

## Acceptance

- `g04.035` is active
- `071` is the active strict lane
- contract `027` exists as the anchor for the lane
- `663` is the next ready card
- no implementation code is changed in this opening card

## Outcome

- opened `g04.035`
- added strict lane `071`
- added contract `027`
- selected `663` as the first classification card
- recorded the existing `state_command.rs` worktree boundary
