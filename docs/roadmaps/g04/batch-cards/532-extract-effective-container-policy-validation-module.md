# 532 - Extract Effective Container Policy Validation Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move container policy validation helpers out of `crates/effigy-containers/src/lib.rs`
into a focused policy validation module without changing behavior.

## Scope

- create `crates/effigy-containers/src/policy/validation.rs`
- move validation-oriented helpers where dependencies remain clean:
  - `validate_container_policy`
  - `validate_compose_backend_runtime`
  - `validate_compose_backend_host_paths`
  - `validate_compose_backend_mount_budget`
  - `is_colima_temp_root_path`
  - `estimate_primary_service_mount_label_size`
  - `parse_mount_budget_entry`
  - `path_is_within`
- keep existing public exports stable through `lib.rs`
- preserve all error text

## Non-Goals

- no policy loading split
- no inline workspace split
- no workspace module split
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when validation helpers live under
`policy/validation.rs`, policy validation tests pass, and public callers still
compile.

## Closeout

Policy validation helpers now live under
`crates/effigy-containers/src/policy/validation.rs` and the public validation
functions remain re-exported from `lib.rs`. `lib.rs` dropped from 1255 to 1067
lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-validation-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-validation-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-validation-test cargo test -p effigy-containers -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`533-extract-inline-workspace-policy-module.md`](./533-extract-inline-workspace-policy-module.md).
