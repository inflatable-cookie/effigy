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

- `642` audit the bounded command-reference gaps and land the guide fixes

## Execution Chain

- `640` complete: opened the lane, promoted the contract anchor, and selected
  the first contract-boundary card
- `641` complete: locked the missing command/flag set, `version` coverage rule, and
  the bounded `--repo` widening for `changelog` and `bundle`
- `642` ready: audit the live parser/help surface against the command matrix
  and land the guide-only reference fixes before repo-targeting widening starts

## Exit Condition

This lane is complete when the command reference matrix matches the live parser
for the bounded gaps in scope, and the repo-local `changelog` and `bundle`
surfaces accept `--repo <PATH>` with focused parser and runner proof coverage.

## Next Task

Execute `642` to land the pure guide/reference fixes before parser or runner
changes start.
