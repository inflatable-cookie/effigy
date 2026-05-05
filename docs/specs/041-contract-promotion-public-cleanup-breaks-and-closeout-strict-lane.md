# 041 - Contract Promotion Public Cleanup Breaks And Closeout Strict Lane

Roadmap: [`g03.035`](../roadmaps/g03/035-contract-promotion-public-cleanup-breaks-and-closeout.md)

Status: Active
Owner: Platform
Created: 2026-05-05

## Purpose

Promote the modularisation work from `g03.030` through `g03.034` into durable
contracts, architecture ownership, and cleanup-break notes.

## Hard Boundaries

- do not edit `.github/workflows/`
- do not initiate release commands
- keep changelog entries scoped to user-facing cleanup breaks only
- do not add public JSON schemas unless a card deliberately promotes manager
  reports or execution plans as public CLI output
- prefer contract and package-map updates over new planning prose

## Promotion Areas

- widen `005-container-runtime-contract.md` for manager-backed runtime
  operation ownership
- widen `009-execution-surface-convergence.md` for context/request-builder
  authority
- keep `011-runtime-context-contract.md` aligned with shipped context behavior
- keep `012-container-manager-contract.md` aligned with shipped manager
  behavior
- add `013-task-execution-request-contract.md`
- update the architecture package map for `effigy-context`,
  `effigy-container-manager`, and `effigy-execution`
- decide whether any cleanup breaks need `CHANGELOG.md`

## Current Ready Card

[`414-close-contract-promotion-and-modularisation-round.md`](./batch-cards/414-close-contract-promotion-and-modularisation-round.md)

## Exit Condition

This lane closes when the contract set and package map name the shipped
runtime/context/container/execution ownership, any intentional public cleanup
breaks are documented, and the g03 modularisation round has no stale ready card.

## Next Task

Complete card `414`.
