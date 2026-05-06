# 416 - Scaffold Effigy Artifacts Crate

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-06
Completed: 2026-05-06

## Goal

Add the first dependency-light `effigy-artifacts` crate with the core artifact
types, local/OCI reference parsing, metadata model, staging report model, and
focused unit tests.

## Scope

- add `crates/effigy-artifacts`
- wire it into the workspace
- define artifact source refs:
  - local path
  - explicit `oci://` ref
- define artifact kinds
- define `effigy.artifact.v1` metadata
- define staged artifact reports
- define operation report shells for future apply/capture integration
- add tests for local SQL refs, local compressed SQL refs, local dump refs,
  `oci://` refs, and unsupported ambiguous registry-looking strings

## Non-Goals

- no OCI network pull/push yet
- no bootstrap/container data integration yet
- no public CLI command yet
- no Acowtancy file edits
- no migration semantics in Effigy

## Exit Condition

This card is complete when `cargo test -p effigy-artifacts` passes and the
crate exposes stable models for the local staging and OCI implementation cards
to consume.

## Closeout

Added `crates/effigy-artifacts` and wired it into the workspace.

The crate now models:

- local and explicit `oci://` artifact refs
- artifact kinds
- `effigy.artifact.v1` metadata
- staged artifact reports
- operation report shells for future apply/capture integration

Validation passed:

```sh
cargo test -p effigy-artifacts
```

## Next Task

Card [`417-local-artifact-staging-for-seed-inputs.md`](./417-local-artifact-staging-for-seed-inputs.md).
