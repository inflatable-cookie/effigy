# Scan God-Files Doctor JSON Bridge Validation

Status: complete
Created: 2026-03-05
Roadmap: g012.000
Batch: scan-god-files-doctor-json-bridge

## Summary
- Validated that `scan.god-files` findings propagate into `effigy doctor` JSON output.
- Validated that doctor severity grouping reflects scanner severity escalation while preserving per-finding evidence labels.
- Validated that `[scan.god_files].doctor = false` suppresses the scanner from doctor JSON payloads.

## Changes
- Added doctor JSON contract coverage in `src/tests/json_contract_tests/doctor_contract_tests.rs`.

## Validation Performed
- command: `cargo test doctor_json_contract_`
  - result: pass

## Risks
- Doctor JSON currently exposes scan findings as grouped doctor findings rather than embedding the raw `effigy.scan.god-files.v1` payload. If downstream consumers later need raw scan metadata inside doctor output, that will require an additive schema extension.

## Next Task
- Run a broader scan feature closeout regression batch across scan, doctor, help/schema, and CLI envelope surfaces, then decide whether any remaining polish belongs in this feature or a follow-up scanner expansion batch.
