# Validation Gap And Hotspot Scans

Date: 2026-05-25
Roadmap: `g08.006`
Batch: `1034`

## What Changed

- added `effigy scan validation-gaps`
- added `[scan.validation_gaps]` with:
  - `hotspot_threshold`
  - `affected_depth`
  - `allow_paths`
  - `include_heuristic`
- added changed-path mode with `--path` and `--stdin`
- reused `graph affected` evidence for likely test files and likely test tasks
- kept findings advisory and split likely tests from missing-test findings

## Validation

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1034b cargo test -p effigy-manifest scan_config_accepts_validation_gap_settings -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1034b cargo test -p effigy-builtin parse_scan_request_accepts_validation_gaps_changed_path_flags -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1034b cargo test -p effigy run_manifest_task_builtin_scan_validation_gaps_reports_changed_owner_without_tests -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1034b cargo test -p effigy run_manifest_task_builtin_scan_validation_gaps_surfaces_likely_tests_for_changed_owner -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1034b cargo test -p effigy builtin_scan_validation_gaps_json_contract_reports_changed_owner_findings -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1034b cargo test -p effigy run_manifest_task_builtin_config_schema_target_scan_prints_god_files_section -- --nocapture`

## Vision Target Delta

- primary tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved: graph-aware scan lane now covers validation risk, changed-file narrowing, and graph-backed likely test suggestions
- remains open: `1035` agent docs, JSON examples, and benchmark proof; `1036` closeout
