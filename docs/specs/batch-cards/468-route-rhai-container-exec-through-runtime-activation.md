# 468 - Route Rhai Container Exec Through Runtime Activation

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make Rhai container execution go through runtime activation planning before
running `docker compose exec`.

## Scope

- update `src/runner/script_command/mod.rs` container exec callbacks
- keep `exec::run(..., #{ run_in: "container", ... })` as the preferred Rhai
  surface
- ensure the callback activates the selected container policy before exec
- preserve `stdin_file` behavior for SQL/import-style scripts
- keep first-party Rhai drift guards intact

## Non-Goals

- no Rhai host API split in this card
- no public Rhai function rename
- no broad container command refactor
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when Rhai container exec cannot bypass runtime activation
and the DecodeLabs-style mysql seed route remains covered.

## Closeout

Rhai container exec callbacks now activate the selected container runtime before
calling container exec capture.

This keeps `exec::run(..., #{ run_in: "container", ... })` on the execution
builder path while closing the runner callback gap that could skip runtime prep
for container-sensitive scripts.

## Validation

- `cargo test -p effigy-rhai`
- `cargo test -p effigy --lib script_command`
- `cargo test -p effigy --lib container_runtime_prep`
- `git diff --check`

## Next Task

Decide whether `g04.003` can close or needs activation report plumbing before
handoff to `g04.004`.
