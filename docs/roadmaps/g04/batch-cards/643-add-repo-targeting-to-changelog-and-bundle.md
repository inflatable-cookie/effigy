# 643 - Add Repo Targeting to Changelog and Bundle

Roadmap: [`../024-command-reference-completeness-and-flag-consistency.md`](../024-command-reference-completeness-and-flag-consistency.md)
Strict lane: [`../../../specs/067-command-reference-completeness-and-flag-consistency-strict-lane.md`](../../../specs/067-command-reference-completeness-and-flag-consistency-strict-lane.md)
Contract: [`../../../contracts/022-command-reference-completeness-and-flag-consistency-contract.md`](../../../contracts/022-command-reference-completeness-and-flag-consistency-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Purpose

Add the bounded `--repo <PATH>` widening for repo-local `changelog` and
`bundle` surfaces, then prove the new parser and runner paths.

## Scope

- add `--repo` to `changelog validate|format|analyze|extract`
- add `--repo` to `bundle list|inspect|export`
- thread repo targeting through parser, help, and runner dispatch
- add focused parser and runner proofs
- update the command matrix and help surfaces to reflect the widened parser

## Acceptance

- the bounded `changelog` and `bundle` surfaces accept `--repo <PATH>`
- omitted `--repo` keeps current behavior
- parser/help/runner proofs cover the new paths
- the command reference matrix matches the widened parser
