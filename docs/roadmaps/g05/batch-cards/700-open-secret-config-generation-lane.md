# 700 - Open Secret Config Generation Lane

Roadmap: [`../001-secret-and-local-config-contract.md`](../001-secret-and-local-config-contract.md)
Strict lane: [`../../../specs/076-secret-and-local-config-contract-strict-lane.md`](../../../specs/076-secret-and-local-config-contract-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Open `g05` as the secret and local configuration management generation.

## Scope

- create the active strict lane
- confirm `g05.001` is contract-only
- keep parser, vault, and runtime injection work blocked until contract
  promotion closes
- make `701` the next contract-promotion card

## Acceptance

- active spec `076` exists
- `g05` front doors point at the secret/config generation
- no implementation work starts from this card

## Outcome

- opened strict lane `076`
- anchored the lane on contract `032`
- kept implementation blocked behind contract promotion

## Next Task

Execute `701` to promote the secret/local config contract.

