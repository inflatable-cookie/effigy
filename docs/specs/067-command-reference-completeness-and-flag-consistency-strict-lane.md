# 067 - Command Reference Completeness and Flag Consistency Strict Lane

Roadmap: [`g04.024`](../roadmaps/g04/024-command-reference-completeness-and-flag-consistency.md)

Status: Active
Owner: Platform
Created: 2026-05-10

## Purpose

Close the remaining command-reference drift and add the bounded `--repo`
support needed for repo-local `changelog` and `bundle` surfaces.

## Hard Boundaries

- no new command families
- no container behavior changes
- no bundle-source behavior changes
- no changelog behavior changes beyond repo targeting
- no `.github/workflows/` edits
- no release execution

## Current Ready Card

- `643` add the bounded `--repo` widening for changelog and bundle

## Execution Chain

- `640` complete: opened the lane, promoted the contract anchor, and selected
  the first contract-boundary card
- `641` complete: locked the missing command/flag set, `version` coverage rule, and
  the bounded `--repo` widening for `changelog` and `bundle`
- `642` complete: audited the live parser/help surface against the command
  matrix, fixed the bounded guide-only drift, and cleaned up the nearby
  `docs check <KIND>` matrix drift left from `g04.023`
- `643` ready: add the bounded `--repo` widening for `changelog` and `bundle`
  plus focused parser and runner proofs

## Exit Condition

This lane is complete when the command reference matrix matches the live parser
for the bounded gaps in scope, and the repo-local `changelog` and `bundle`
surfaces accept `--repo <PATH>` with focused parser and runner proof coverage.

## Next Task

Execute `643` to add the bounded `--repo` widening for `changelog` and
`bundle`.
