# Duplicate Blocks Docs And Benchmark Validation

Status: complete
Created: 2026-03-06
Roadmap: g01.016
Batch: duplicate-blocks-docs-and-benchmark-validation

## Summary
- Updated the operator docs, manifest cookbook, JSON examples, CI recipes, snippets, and schema index to include `effigy scan duplicate-blocks`.
- Validated `duplicate-blocks` doctor integration and CLI JSON envelope coverage.
- Benchmarked the scanner on `acowtancy` to decide whether doctor participation should remain opt-in by default.

## Changes
- Added `duplicate-blocks` coverage to:
  - `docs/guides/017-json-output-contracts.md`
  - `docs/guides/021-quick-start-and-command-cookbook.md`
  - `docs/guides/022-manifest-cookbook.md`
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/guides/025-command-reference-matrix.md`
  - `docs/guides/026-json-payload-examples.md`
  - `docs/guides/027-copy-paste-snippets.md`
  - `docs/contracts/json-schema-index.json`
- Closed roadmap `g01.016` and recorded the benchmark decision to keep `[scan.duplicate_blocks].doctor = false` as the default.

## Vision Target Delta
- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: baseline `duplicate-blocks existed in code/contracts but not in the user-facing docs or real-repo validation record` -> current `duplicate-blocks is documented across operator surfaces, indexed in schema references, and benchmarked on a real repo with an explicit default-doctor policy decision`
- Remaining gap: `None`

## Validation Performed
- command: `CARGO_TARGET_DIR=target-codex-dup2 cargo test run_doctor_reports_duplicate_blocks_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-dup2 cargo test doctor_json_contract_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-dup2 cargo test doctor_check_registry_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-dup2 cargo test cli_json_mode_scan_duplicate_blocks_ --test cli_output_tests`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-dup2 cargo test scan_contract_tests --lib`
  - result: passed
- command: `bash docs/scripts/check-vision-metadata.sh`
  - result: passed
- command: `target-codex-dup2/debug/effigy scan duplicate-blocks`
  - result: completed in `16.85s` with `scanned-files=1905`, `candidate-blocks=207604`, `findings=95`

## Risks
- `duplicate-blocks` remains materially slower and noisier than the other scanners on a large real repo.
- Leaving doctor participation opt-in is the correct default until either detection is narrowed further or runtime drops materially.

## Next Task
- Roadmap `g01.016` is complete. Choose the next scanner milestone only after deciding whether to prioritize cleanup heuristics or repo-policy checks.
