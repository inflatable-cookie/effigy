# 022 - Command Reference Completeness and Flag Consistency Contract

Status: Active
Owner: Platform
Updated: 2026-05-10

## Purpose

Lock the boundary for closing the remaining command-reference and flag-shape
drift before implementation starts.

## Scope

This contract owns:

- the missing `version` command reference entry
- missing container command shapes and flags in the command matrix
- the bounded `--repo` widening for `changelog`
- the no-behavior-change rule for command surfaces other than the added
  repo-target override

This contract does not own:

- new command families
- container behavior changes
- bundle-source behavior changes
- changelog parsing or formatting behavior changes

## Reference Completeness Rule

The canonical reference guide must cover the live parser surface for:

- `effigy version`
- `effigy container cache prune`
- `effigy container volume prune`
- the missing `--project` and `--kind` flags on `container cache list`
- the missing `--push` flag on `container data dump`

This lane is allowed to add missing reference rows and flags. It is not allowed
to reshape existing command meaning.

## `version` Rule

The guide must document both:

- the bare command `effigy version`
- the global alias `effigy --version`

They are the same operator surface, presented in two parser forms.

## `--repo` Widening Rule

`--repo <PATH>` may be added only where the command already operates on
repo-local files or repo-root-relative destinations.

Bounded widening for this lane:

- `changelog validate`
- `changelog format`
- `changelog analyze`
- `changelog extract`

The widened commands must keep the same behavior when `--repo` is omitted.

## Bundle Boundary

This lane touched `bundle` only while the old surface still existed. The
current bundle surface is outside scope here.

## Changelog Boundary

This lane may add repo-targeting to changelog surfaces. It may not widen
changelog schema, release orchestration, or version-selection behavior.

## Acceptance

- the command reference matrix covers the live parser surface named above
- `changelog` accepts `--repo <PATH>` on the bounded subcommands
- text/help/reference surfaces align with the widened parser
- focused parser/runner proofs cover the new `--repo` paths
