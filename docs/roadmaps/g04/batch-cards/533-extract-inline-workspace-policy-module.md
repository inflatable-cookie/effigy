# 533 - Extract Inline Workspace Policy Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move inline workspace policy helpers out of `crates/effigy-containers/src/lib.rs`
into a focused policy module without changing behavior.

## Scope

- create `crates/effigy-containers/src/policy/inline_workspace.rs`
- move inline-workspace helpers where dependencies remain clean:
  - `load_inline_workspace_container_policy`
  - `resolve_inline_workspace_exec_working_dir`
  - `inline_workspace_compose_mount`
  - inline workspace compose rendering helpers
- keep public exports stable through `lib.rs`
- preserve generated compose and error text

## Non-Goals

- no general policy loading split
- no workspace.rs split
- no runtime DNS split unless required by dependency cleanup
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when inline workspace policy logic lives outside
`lib.rs`, existing inline workspace tests pass, and public callers still
compile.

## Closeout

Inline workspace policy helpers now live under
`crates/effigy-containers/src/policy/inline_workspace.rs` and the public helper
exports remain stable through `lib.rs`. `lib.rs` dropped from 1067 to 885
lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-inline-workspace-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-inline-workspace-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-inline-workspace-test cargo test -p effigy-containers -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`534-extract-runtime-dns-policy-module.md`](./534-extract-runtime-dns-policy-module.md).
