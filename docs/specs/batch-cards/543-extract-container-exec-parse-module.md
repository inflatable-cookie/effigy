# 543 - Extract Container Exec Parse Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Ready
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

- `cargo check -p effigy-containers`
- `cargo check -p effigy --lib`
- `cargo test -p effigy-containers parse_running_compose_containers_splits_tab_fields -- --test-threads=1`
- `cargo test -p effigy-containers parse_running_container_stats_reads_json_lines -- --test-threads=1`
- `cargo test -p effigy-containers infer_host_working_dir_from_inspect_maps_container_working_dir_through_bind_mount -- --test-threads=1`
- `git diff --check`

## Next Task

Extract the container exec parse module.
