# Scan God-Files JSON Contract Validation

Status: complete
Created: 2026-03-05
Roadmap: g012.000
Batch: scan-god-files-json-contract

## Summary
- Added payload-level JSON contract tests for `effigy.scan.god-files.v1`.
- Added top-level CLI envelope tests for `effigy --json scan god-files`.
- Registered the new command surface in `docs/contracts/json-schema-index.json`.

## Changes
- Added `src/tests/json_contract_tests/scan_contract_tests.rs`.
- Added CLI envelope coverage in `tests/cli_output_tests/json_envelope_tests.rs`.
- Added schema index entry for `effigy --json scan god-files`.

## Validation Performed
- command: `cargo test scan_contract_tests`
  - result: pass
- command: `cargo test cli_json_mode_scan_`
  - result: pass

## Risks
- The JSON payload still includes rendered `text` for both text and markdown formats; if a future machine contract wants a structured markdown/text split, this schema will need an additive extension.

## Next Task
- Add JSON contract coverage for doctor output when `scan.god-files` findings are present, so the scanner-to-doctor bridge is validated in both payloads.
