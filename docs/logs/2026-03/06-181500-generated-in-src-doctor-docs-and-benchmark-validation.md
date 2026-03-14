# Generated-In-Src Doctor Docs And Benchmark Validation

Status: complete
Created: 2026-03-06
Roadmap: g01.018
Batch: 18.3-18.4

## Summary
- Added `generated-in-src` integration to `effigy doctor`.
- Updated docs/contracts for the new scan and doctor default.
- Benchmarked the scanner on `acowtancy` to validate runtime and default doctor participation.

## Changes
- Added doctor check registration, progress label, JSON integration, and report-file support for `scan.generated-in-src`.
- Updated quick-start, manifest cookbook, command matrix, JSON contracts, CI recipes, JSON examples, and snippets.
- Added the `effigy --json scan generated-in-src` schema index entry.
- Marked roadmap `g01.018` complete and updated roadmap indexes.

## Vision Target Delta
- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: standalone generated-in-src scanner -> integrated scan plus doctor-backed source-tree hygiene policy
- Remaining gap: None

## Validation Performed
- command: `CARGO_TARGET_DIR=target-codex-generated-in-src cargo test run_doctor_reports_generated_in_src_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-generated-in-src cargo test doctor_json_contract_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-generated-in-src cargo test doctor_check_registry_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-generated-in-src cargo test run_manifest_task_builtin_scan_generated_in_src_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-generated-in-src cargo test cli_json_mode_scan_generated_in_src_ --test cli_output_tests`
  - result: passed
- command: `bash docs/scripts/check-vision-metadata.sh`
  - result: passed
- command: `command time -p ./target-codex-generated-in-src/debug/effigy scan generated-in-src`
  - result: `scanned-files=1716`, `candidate-files=4`, `findings=4`, `real=2.06s`

## Risks
- Generated client code intentionally checked into source trees may still require per-repo `exclude` or narrower `source_roots`.
- The scanner currently relies on deterministic marker/path heuristics rather than language-aware codegen provenance.

## Next Task
- Roadmap `g01.018` is complete. Choose the next scan milestone before opening more implementation work.
