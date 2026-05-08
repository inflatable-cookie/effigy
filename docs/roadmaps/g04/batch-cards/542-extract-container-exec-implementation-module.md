# 542 - Extract Container Exec Implementation Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting `crates/effigy-containers/src/exec.rs` by moving the current
implementation behind a small public module facade without changing exported
container exec APIs.

## Scope

- create `crates/effigy-containers/src/exec/implementation.rs`
- keep `exec.rs` as a public facade that re-exports the existing API
- preserve all current public function and type names
- avoid behavior changes while preparing later smaller exec module splits

## Non-Goals

- no manager migration
- no Colima recovery behavior changes
- no runner call-site migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `exec.rs` is no longer a god file, existing public
exec imports still compile, and focused exec/container tests still pass.

## Validation

- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-impl-check cargo check -p effigy-containers`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-impl-libcheck cargo check -p effigy --lib`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-impl-test-a cargo test -p effigy-containers parse_running_compose_containers_splits_tab_fields -- --test-threads=1`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-impl-test-b cargo test -p effigy-containers parse_running_container_stats_reads_json_lines -- --test-threads=1`
- PASS: `git diff --check`

Note: `cargo check -p effigy --lib` still reports the pre-existing
`runtime_activation_report_for_result` dead-code warning.

## Next Task

Start
[`543-extract-container-exec-parse-module.md`](./543-extract-container-exec-parse-module.md).
