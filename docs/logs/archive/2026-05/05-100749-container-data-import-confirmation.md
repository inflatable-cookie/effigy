# Container Data Import Confirmation

Date: 2026-05-05
Roadmap: `g03.027`
Batch Card: `366-implement-container-data-import-confirmation.md`

## Summary

`container data import` now follows the shared prompt policy before importing
an archive into generated-compose data. TTY runs prompt with a default-no
confirmation; `--json` and redirected I/O fail clearly unless automation passes
`--yes`.

## Changed

- added `--yes` parsing for `effigy container [<NAME>] data import <VOLUME> <PATH>`
- added runner-side import confirmation before runtime import side effects
- kept direct-compose ownership rejection ahead of prompt evaluation
- updated help, command-reference, container guide, changelog, and active
  roadmap/spec pointers

## Validation

- `cargo check -p effigy-cli`
- `cargo check -p effigy`
- `cargo test -p effigy --lib parse_container_data_import -- --nocapture`
- `cargo test -p effigy --lib prompt_container_data_import -- --nocapture`
- `cargo test -p effigy --lib container_data_import_prompt -- --nocapture`
- `cargo test -p effigy --lib run_container_data_import_rejects_direct_compose_ownership -- --nocapture`

## Vision Target Delta

Primary tags: `CONTRACT`, `OPERATE`, `MAINT`

Baseline: `container data import` could overwrite local generated-compose data
without a prompt-specific guard.

Current state: import now uses the same prompt policy as the preceding
destructive prompt surfaces and has an explicit `--yes` automation path.

Remaining open: broad `unlock` confirmation still needs a boundary decision
before implementation.
