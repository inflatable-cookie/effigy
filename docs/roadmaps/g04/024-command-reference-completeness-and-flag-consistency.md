# 024 - Command Reference Completeness and Flag Consistency

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-10
Depends on:
- [`023-docs-check-subcommand-consolidation.md`](./023-docs-check-subcommand-consolidation.md)

## Goal

Close gaps between the CLI implementation and the canonical reference guide.
Add missing `--repo` support where appropriate.

## Scope

### 1. Document `version` Command

`effigy version` and `effigy --version` exist in the parser but are missing
from `docs/guides/025-command-reference-matrix.md`.

- Add `version` to Primary Commands table
- Add Command Shape entry
- Note: `--version` is a global flag alias, `version` is a bare command that
  prints the same information

### 2. Fix Missing Container Subcommand Shapes

The guide documents `container cache list` and `container volume list` but omits
`container cache prune` and `container volume prune`.

- Add `container cache prune [NAME] --global --yes --project --kind --repo --json`
- Add `container volume prune [NAME] --global --yes --orphans --dormant --repo --json`

Also fix missing flags on documented shapes:
- `container cache list` is missing `--project` and `--kind` flags
- `container data dump` is missing `--push` flag

### 3. Add `--repo` to `changelog` and `bundle`

Most commands support `--repo`. `changelog` and `bundle` do not, despite
operating on repo-local files.

- Add `--repo` to `changelog validate/format/analyze/extract`
- Add `--repo` to `bundle list/inspect/export`

Note: `bundle` commands operate on shipped bundles, but `bundle export --path`
writes to a repo-local directory. `--repo` anchors the export destination.

## Non-Goals

- No changes to command behavior beyond adding `--repo` support
- No changes to internal architecture
- No `.github/workflows/` edits
- No release execution

## Why Now

Reference gaps create confusion. Users discover commands via `--help` that are
not documented, or documented commands that lack flags the parser actually
accepts. Closing these gaps is low-risk, high-trust work.

## Core Decisions

### `--repo` Addition Criteria

A command should support `--repo` if it resolves files relative to the repo
root. `changelog` reads `CHANGELOG.md` (repo-relative). `bundle export` writes
to a repo-relative path. Both qualify.

### Guide Update Process

The command reference matrix is manually maintained. This roadmap audits it
against the parser definitions in `crates/effigy-cli/src/`.

## Success Criteria

- `version` appears in Primary Commands table and Command Shapes
- `container cache prune` and `container volume prune` appear in Command Shapes
- `container cache list --project/--kind` documented
- `container data dump --push` documented
- `changelog` commands accept `--repo`
- `bundle` commands accept `--repo`
- Parser and guide are consistent
- Changelog entry under `[Unreleased] Changed`

## Suggested Batch Order

1. Audit parser definitions against guide, list all gaps
2. Update guide with missing commands/flags
3. Add `--repo` to CLI structs for `changelog` and `bundle`
4. Wire `--repo` through runner dispatch
5. Add tests for `--repo` on new commands

## Validation

- `effigy changelog --repo <PATH> validate` works
- `effigy bundle --repo <PATH> export underlay --path bundles/underlay` works
- Guide passes link check
- `git diff --check`

## Next Task

Execute `642` to audit the command matrix against the live parser and land the
bounded guide-only fixes before `--repo` widening starts.
