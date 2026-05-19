# 1006 - Open Codebase Leanness Lane

Roadmap: [`../056-codebase-leanness-and-boundary-hardening-suite.md`](../056-codebase-leanness-and-boundary-hardening-suite.md)
Strict lane: [`../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md`](../../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Open the cleanup lane from the reusable codebase sweep audit and capture the
baseline evidence before changing code.

## Work

- confirm current dirty worktree and protect unrelated changes
- rerun or record baseline scan outputs:
  - `effigy scan god-files --json`
  - `effigy scan duplicate-blocks --json`
  - `effigy scan attention-markers --json`
  - `effigy scan comment-ratio --json`
- confirm `effigy test --plan`
- update lane state only after the baseline is clear

## Guardrails

- do not start cleanup implementation in this card
- do not run release mutations
- do not edit workflows
- do not discard existing init-related changes

## Acceptance

- baseline evidence is known
- next implementation target is `1007`
- no unrelated worktree changes are reverted

## Evidence

- dirty worktree confirmed as planning-only files for the new `g07.056` lane
- `effigy scan god-files --json`: 4 findings
- `effigy scan duplicate-blocks --json`: 110 findings
- `effigy scan attention-markers --json`: 0 findings
- `effigy scan comment-ratio --json`: 0 findings
- `effigy test --plan`: `cargo nextest run`

## Next Task

Execute `1007`.
