# Doctor Explain Mode Release Note

Date: 2026-02-28
Owner: Platform
Related roadmap: 009 - Doctor Health Consolidation

## Summary

`doctor` now includes task-resolution explain mode:

- `effigy doctor <task> <args>`
- `effigy --json doctor <task> <args>`

Explain mode is part of the existing `doctor` command surface and does not introduce a new top-level command.

## What Changed

- Added explain-mode invocation parsing for `doctor`:
  - first non-flag token after `doctor` becomes the explain target task selector.
  - remaining tokens are treated as diagnosed request arguments.
- Added text explain report with:
  - candidate catalog list,
  - selected catalog/mode/evidence,
  - ambiguity candidate reporting (when applicable),
  - deferral considered/selected/source details.
- Added JSON explain payload:
  - schema: `effigy.doctor.explain.v1`
  - includes `selection`, `deferral`, `candidates`, `ambiguity_candidates`, and `reasoning`.
- Added explicit reasoning parity fields:
  - text: `selection-reasoning`, `deferral-reasoning`
  - json: `reasoning.selection`, `reasoning.deferral`
- Added guardrail:
  - `--fix` is rejected when explain mode is used.

## Command Examples

```bash
effigy doctor farmyard/build -- --watch
effigy --json doctor farmyard/build -- --watch
```

## Validation

- `cargo test run_doctor_ -- --nocapture`
  - result: pass
- `cargo test doctor_explain_json_contract_has_selection_and_deferral_fields -- --nocapture`
  - result: pass
- `cargo test doctor_explain_json_snapshot_prefix_is_stable -- --nocapture`
  - result: pass
- `cargo test run_doctor_explain_text_snapshot_prefix_block_is_stable -- --nocapture`
  - result: pass
- `cargo test --lib`
  - result: pass (`236 passed, 0 failed`)

## Compatibility

- Existing `effigy doctor` health/remediation behavior is unchanged.
- Existing `effigy --json doctor` health payload (`effigy.doctor.v1`) is unchanged.
- Explain mode is additive and uses a distinct payload schema (`effigy.doctor.explain.v1`).
