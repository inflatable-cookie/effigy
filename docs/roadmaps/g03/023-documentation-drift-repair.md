# 023 - Documentation Drift Repair

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-03
Depends on: —

## Problem

The primary user-facing install instruction in `README.md` references an outdated
tag: `--tag v0.3.0` when the current workspace version is `0.3.3`.

Secondary drift exists in JSON examples and onboarding docs that still show
`v0.3.1` or older version strings. These mislead new users and create support
friction.

## Goal

Repair version drift in user-facing documentation so install instructions and
examples match the current release.

## Scope

- update `README.md` install command to the current tag
- sweep `docs/guides/` for stale version references in copy-pasteable examples
- update `docs/guides/017-json-output-contracts.md` and
  `026-json-payload-examples.md` example version strings
- update `docs/guides/030-contributor-onboarding-15-minutes.md` version
  references
- leave historical records (roadmaps, logs, changelogs) untouched

## Non-Goals

- rewriting documentation content
- updating `CHANGELOG.md` entries (they are historical)
- bumping version strings in code or config

## Exit Condition

This milestone is complete when a new user can copy-paste the README install
command and get the current version, and no JSON example in `docs/guides/`
shows a stale version string.

## Next Task

If this lane is promoted, start by grepping for version strings in `README.md`
and `docs/guides/`.
