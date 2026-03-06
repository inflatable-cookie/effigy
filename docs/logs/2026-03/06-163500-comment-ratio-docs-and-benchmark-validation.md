# Comment Ratio Docs and Benchmark Validation

Status: complete
Created: 2026-03-06
Roadmap: g01.017
Batch: comment-ratio-docs-and-benchmark-validation

## Summary
- Updated the operator docs, manifest cookbook, JSON contract guide, snippets, schema index, and roadmap indexes to include `effigy scan comment-ratio`.
- Validated standalone scan contracts, doctor integration, CLI JSON envelope coverage, and the schema/help surfaces.
- Benchmarked `comment-ratio` on `acowtancy` and used the result to make a default-doctor decision.

## Changes
- Added `comment-ratio` coverage to:
  - `docs/guides/017-json-output-contracts.md`
  - `docs/guides/021-quick-start-and-command-cookbook.md`
  - `docs/guides/022-manifest-cookbook.md`
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/guides/025-command-reference-matrix.md`
  - `docs/guides/026-json-payload-examples.md`
  - `docs/guides/027-copy-paste-snippets.md`
  - `docs/contracts/json-schema-index.json`
- Closed roadmap `g01.017` and updated roadmap indexes.
- Switched the scanner default to `[scan.comment_ratio].doctor = true` after the benchmark validated runtime/noise.

## Vision Target Delta
- Primary tags: `OPERATE`, `CONTRACT`, `RELEASE`
- Movement: baseline `comment-ratio existed in code/contracts but was not documented or benchmarked` -> current `comment-ratio is documented across operator surfaces, indexed in schema references, benchmarked on a real repo, and has an explicit default-doctor policy decision`
- Remaining gap: `None`

## Validation Performed
- command: `CARGO_TARGET_DIR=target-codex-comment cargo test run_manifest_task_builtin_scan_comment_ratio_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-comment cargo test scan_contract_tests --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-comment cargo test doctor_json_contract_ --lib`
  - result: passed
- command: `CARGO_TARGET_DIR=target-codex-comment cargo test cli_json_mode_scan_comment_ratio_ --test cli_output_tests`
  - result: passed
- command: `command time -p target-codex-comment/debug/effigy scan comment-ratio --repo /Users/betterthanclay/Dev/projects/acowtancy`
  - result: `scanned-files=1905`, `candidate-files=1472`, `findings=15`, `real=2.41s`
- command: `bash docs/scripts/check-vision-metadata.sh`
  - result: passed

## Risks
- Comment classification remains heuristic for unsupported extensions; unknown types still fall back conservatively.
- Some teams may prefer `doctor = false` if commentary-heavy tests are intentionally verbose, but the benchmark did not justify making that the default globally.

## Next Task
- Pick the next scan milestone and create the roadmap before implementation work starts.
