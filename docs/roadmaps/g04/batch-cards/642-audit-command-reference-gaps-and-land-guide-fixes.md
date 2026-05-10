# 642 - Audit Command Reference Gaps and Land Guide Fixes

Roadmap: [`../024-command-reference-completeness-and-flag-consistency.md`](../024-command-reference-completeness-and-flag-consistency.md)
Strict lane: [`../../../specs/067-command-reference-completeness-and-flag-consistency-strict-lane.md`](../../../specs/067-command-reference-completeness-and-flag-consistency-strict-lane.md)
Contract: [`../../../contracts/022-command-reference-completeness-and-flag-consistency-contract.md`](../../../contracts/022-command-reference-completeness-and-flag-consistency-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Purpose

Audit the live parser and help surfaces against the command matrix, then land
 the pure reference fixes before the `--repo` widening batch starts.

## Scope

- add the missing `version` command reference entry
- add the missing `container cache prune` and `container volume prune` shapes
- add the missing `--project`, `--kind`, and `--push` flags in the guide
- update help/reference wording if the guide audit exposes nearby drift
- keep this batch guide-only; no parser or runner behavior changes

## Acceptance

- `025-command-reference-matrix.md` matches the bounded live parser surface in scope
- guide/help drift for the audited commands is resolved
- the next batch can focus only on `--repo` widening for `changelog` and `bundle`
