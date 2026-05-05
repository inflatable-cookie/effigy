# 035 - v0.x Release Readiness Audit and Gate Alignment Strict Lane

Roadmap: [`g03.029`](../roadmaps/g03/029-v0-x-release-readiness-audit-and-gate-alignment.md)

Status: Active
Owner: Platform
Created: 2026-05-05

## Purpose

Audit current release readiness without initiating a release.

This lane exists because the repo has accumulated substantial `v0.x` work
after the last release-readiness assessment. The next useful move is to prove
whether the live release contract, gates, changelog, and install guidance still
line up before a human asks for a release flow.

## Hard Boundaries

- do not run `effigy release prepare --yes`
- do not run `effigy release execute`
- do not create or move tags
- do not edit `.github/workflows/`
- do not treat audit success as release approval

## Allowed Checks

- read release guides, contracts, changelog, and task manifests
- run non-destructive planning or status commands
- run focused tests only when they validate a discovered readiness risk
- run `git diff --check`

## Current Ready Card

[`373-audit-v0-x-release-readiness-and-gate-alignment.md`](./batch-cards/373-audit-v0-x-release-readiness-and-gate-alignment.md)

## Exit Condition

This lane closes when the release-readiness audit is logged and any small
readiness-doc drift found during the audit is repaired.

## Next Task

Execute [`373-audit-v0-x-release-readiness-and-gate-alignment.md`](./batch-cards/373-audit-v0-x-release-readiness-and-gate-alignment.md).
