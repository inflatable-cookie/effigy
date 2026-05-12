# 701 - Promote Secret Config Contract

Roadmap: [`../001-secret-and-local-config-contract.md`](../001-secret-and-local-config-contract.md)
Strict lane: [`../../../specs/076-secret-and-local-config-contract-strict-lane.md`](../../../specs/076-secret-and-local-config-contract-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Promote the secret and local configuration contract as the governing boundary
for the `g05` generation.

## Scope

- mark contract `032` active
- close `g05.001`
- record the default unlock posture
- confirm SSH-key-only unlock is not the default
- select the next implementation-adjacent step as an audit, not parser work

## Acceptance

- contract `032` is active
- `g05.001` is complete
- strict lane `076` is complete
- `702` is the next ready card
- no parser, vault, or injection implementation has started

## Outcome

- promoted contract `032`
- closed `g05.001`
- confirmed default unlock policies require human participation through
  `passphrase` or `key-and-passphrase`
- left Varlock as an adapter option rather than the central contract

## Next Task

Execute `702` to audit current config and secret boundaries.

