# 575 - Add Runtime Operation Pipeline Contract

Lane: [`053-contract-promotion-and-g04-closeout-strict-lane.md`](../053-contract-promotion-and-g04-closeout-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add the missing contract for the `g04` runtime operation pipeline architecture.

## Scope

- create `docs/contracts/015-runtime-operation-pipeline-contract.md`
- define the four pipeline families:
  - execution pipeline
  - runtime activation pipeline
  - container operation pipeline
  - artifact/data pipeline
- name owning crates and runner adapter boundaries
- document drift guards and proof expectations at contract level
- link the new contract from the package map and contract index if appropriate

## Non-Goals

- no broad rewrites to existing contracts in this card
- no code changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the new contract exists, links cleanly, and gives
the remaining existing-contract updates a single authority to reference.

## Validation

- docs path/link checks for changed contract and architecture files
- `git diff --check`

## Next Task

Start
[`576-align-existing-contracts-with-runtime-operation-pipelines.md`](./576-align-existing-contracts-with-runtime-operation-pipelines.md).
