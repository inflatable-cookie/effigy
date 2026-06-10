# 2026-03-06 09:15 - Scan Attention Markers Envelope and Doctor Validation

Related roadmap: `docs/roadmaps/g01/014-attention-marker-scan-and-doctor-integration.md`

## Scope

Validate the new `attention-markers` scanner across:
- direct scan payload contracts,
- top-level CLI JSON envelope wrapping,
- doctor text/json integration,
- docs workflow metadata checks.

## Validation Commands

```sh
cargo test run_doctor_reports_attention_markers_ --lib
cargo test run_doctor_skips_attention_markers_when_doctor_flag_is_disabled --lib
cargo test doctor_json_contract_ --lib
cargo test run_manifest_task_builtin_scan_ --lib
cargo test run_manifest_task_builtin_config_schema_target_scan_prints_god_files_section --lib
cargo test cli_json_mode_scan_attention_markers_ --test cli_output_tests
bash docs/scripts/check-vision-metadata.sh
```

## Outcome

- `scan attention-markers` passes focused builtin scan tests.
- `effigy doctor` includes `scan.attention-markers` findings when enabled and omits them when `doctor = false`.
- CLI JSON envelope tests now include direct assertions for:
  - `effigy --json scan attention-markers`
  - `effigy --json scan attention-markers --fail-on-findings`
- Docs metadata/workflow checks pass after the guide and schema-index updates.

## Notes

- The scanner keeps warning rows hidden by default in terminal text output but preserves full detail in raw JSON payloads.
- Marker matching was tightened to avoid false positives from Rust attribute arguments such as `#[deprecated(note = ...)]`.

## Next Task

Close roadmap `g01.014` explicitly if no more scanner-polish work remains, or open the next scan roadmap batch for `duplicate-blocks` / `generated-assets` follow-up work.
