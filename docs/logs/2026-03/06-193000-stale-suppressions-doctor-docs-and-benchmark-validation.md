# Stale Suppressions Doctor Docs And Benchmark Validation

Status: complete
Created: 2026-03-06
Roadmap: g01.019
Batch: stale-suppressions-closeout

## Summary
- Closed the stale-suppressions milestone after doctor integration, docs updates, and real-repo benchmark validation.
- Kept `scan.stale_suppressions` doctor participation opt-in by default.
- Aligned config schema docs, user guides, roadmap state, and benchmark outcome.

## Changes
- Set `StaleSuppressionScanOptions::default().doctor_enabled` to `false`.
- Updated the scan config schema/example snippets to show `doctor = false` for `[scan.stale_suppressions]`.
- Updated user-facing scan docs to describe stale-suppressions as an opt-in doctor check.
- Closed roadmap `g01.019` and updated roadmap indexes.

## Benchmark
- Command: `./target/debug/effigy scan stale-suppressions --repo /Users/betterthanclay/Dev/projects/acowtancy`
- Result: `scanned-files: 1905`, `matched-lines: 69`, `findings: 69`
- Severity counts: `critical=0`, `high=64`, `warning=5`
- Runtime: about `3.9s`
- Decision: useful scanner, but too noisy for default `effigy doctor` participation in a large multi-repo workspace.

## Validation
- `cargo test run_doctor_reports_stale_suppressions_ --lib`
- `cargo test doctor_json_contract_ --lib`
- `cargo test doctor_check_registry_ --lib`
- `cargo test run_manifest_task_builtin_config_schema_target_scan_prints_god_files_section --lib`
- `cargo test cli_json_mode_scan_stale_suppressions_ --test cli_output_tests`
- `bash docs/scripts/check-vision-metadata.sh`

## Vision Target Delta
- Primary tags: `scan`, `doctor`, `docs`, `contracts`
- Status: milestone complete
