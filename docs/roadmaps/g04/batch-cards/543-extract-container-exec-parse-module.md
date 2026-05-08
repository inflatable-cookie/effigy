# 543 - Extract Container Exec Parse Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting the container exec implementation by moving Docker/Colima
output parsing into a focused module.

## Scope

- create `crates/effigy-containers/src/exec/parse.rs`
- move parser-owned structs and helpers where dependencies stay clean:
  - running compose container row parsing
  - running container stats parsing
  - inspect working-directory parsing
  - Docker/Colima failure-shape detection helpers if still local to parse tests
- keep public types re-exported through `exec.rs`
- preserve parser behavior and test names where possible

## Non-Goals

- no process execution changes
- no Colima repair behavior changes
- no manager migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when parse-only logic is out of the exec implementation
file, public exec APIs still compile, and focused parser tests pass.

## Validation

- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-parse-check cargo check -p effigy-containers`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-parse-libcheck cargo check -p effigy --lib`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-parse-test-a cargo test -p effigy-containers parse_running_compose_containers_splits_tab_fields -- --test-threads=1`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-parse-test-b cargo test -p effigy-containers parse_running_container_stats_reads_json_lines -- --test-threads=1`
- PASS: `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-parse-test-c cargo test -p effigy-containers infer_host_working_dir_from_inspect_maps_container_working_dir_through_bind_mount -- --test-threads=1`
- PASS: `git diff --check`

Note: `cargo check -p effigy --lib` still reports the pre-existing
`runtime_activation_report_for_result` dead-code warning.

## Next Task

Start
[`544-extract-container-exec-process-module.md`](./544-extract-container-exec-process-module.md).
