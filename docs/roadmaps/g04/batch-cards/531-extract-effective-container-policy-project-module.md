# 531 - Extract Effective Container Policy Project Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move project-name resolution helpers out of `crates/effigy-containers/src/lib.rs`
into a focused policy project module without changing behavior.

## Scope

- create `crates/effigy-containers/src/policy/project.rs`
- move project-name helpers from `lib.rs`:
  - `default_project_name_base`
  - `sanitize_project_name_component`
  - `resolve_project_name`
  - `default_project_name`
  - `validate_unique_project_names`
  - `apply_bootstrap_fresh_session_suffix`
  - `bootstrap_fresh_session_id`
- keep public behavior and error text stable
- keep public exports unchanged

## Non-Goals

- no policy loading split
- no validation split
- no inline workspace split
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when project-name helpers live under
`policy/project.rs`, policy tests still pass, and `lib.rs` no longer owns that
project-name logic.

## Closeout

Project-name helpers now live under
`crates/effigy-containers/src/policy/project.rs`. `lib.rs` dropped from 1379
to 1255 lines while preserving behavior and public exports.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-project-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-project-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-project-test cargo test -p effigy-containers -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`532-extract-effective-container-policy-validation-module.md`](./532-extract-effective-container-policy-validation-module.md).
