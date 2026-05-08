# 541 - Extract Generated Compose Source Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting `crates/effigy-containers/src/policy_support.rs` by moving
generated/direct compose source resolution into a module-owned file without
changing container policy behavior.

## Scope

- create `crates/effigy-containers/src/policy_support/generated_compose.rs`
- keep `policy_support.rs` as a small facade
- move existing generated compose source resolution and related tests intact
- preserve internal exports used by policy loading, validation, tests, and
  workspace host integration

## Non-Goals

- no behavior changes
- no container manager migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `policy_support.rs` is no longer a god file,
generated compose policy tests still pass, and existing internal callers still
compile.

## Validation

- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-generated-compose-check cargo check -p effigy-containers`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-generated-compose-libcheck cargo check -p effigy --lib`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-generated-compose-test-a cargo test -p effigy-containers typed_generated_compose_env_policy_converts_sequence_entries -- --test-threads=1`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-generated-compose-test-c cargo test -p effigy-containers typed_generated_compose_port_policy_rewrites_string_ports -- --test-threads=1`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-generated-compose-test-d cargo test -p effigy-containers typed_generated_compose_mount_policy_detects_repo_root_and_preserves_non_string_volumes -- --test-threads=1`
- PASS: `git diff --check`

Note: `cargo check -p effigy --lib` still reports the pre-existing
`runtime_activation_report_for_result` dead-code warning.

## Next Task

Start
[`542-extract-container-exec-implementation-module.md`](./542-extract-container-exec-implementation-module.md).
