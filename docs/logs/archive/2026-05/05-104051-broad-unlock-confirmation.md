# Broad Unlock Confirmation

Date: 2026-05-05
Roadmap: `g03.027`
Batch Card: `369-implement-broad-unlock-confirmation.md`

## Summary

Broad `unlock` actions now require confirmation before clearing lock scopes.
TTY runs prompt with a default-no confirmation. `--json` and redirected I/O
fail clearly unless automation passes `--yes`.

## Changed

- added `--yes` to `effigy unlock`
- guarded `--all`, `workspace`, `shared:<name>`, and multi-scope unlocks
- kept single `task:<name>` and `profile:<task>/<profile>` unlocks unprompted
- updated Rhai unlock replay to pass `--yes`
- updated help, completion, command docs, troubleshooting docs, JSON docs, and
  changelog
- adjusted prompt policy so explicit automation bypasses can coexist with JSON
  output

## Validation

- `cargo check -p effigy-builtin`
- `cargo check -p effigy`
- `cargo test -p effigy-builtin prompt_policy -- --nocapture`
- `cargo test -p effigy-builtin unlock -- --nocapture`
- `cargo test -p effigy --lib builtin_unlock_parser_contracts_are_stable -- --nocapture`
- `cargo test -p effigy --lib run_manifest_task_builtin_unlock -- --nocapture`
- `cargo test -p effigy --lib run_manifest_task_builtin_help_json_contract_table_has_stable_schema_topic_and_precedence -- --nocapture`
- `cargo test -p effigy --lib run_manifest_task_builtin_help_precedence_contract_table -- --nocapture`
- `cargo test -p effigy --lib run_manifest_task_builtin_entrypoint_help_json_contract_table -- --nocapture`
- `cargo test -p effigy --lib builtin_unlock_json_contract_has_versioned_shape -- --nocapture`
- `cargo test -p effigy --lib rhai -- --nocapture`
- `cargo run -q --bin effigy -- unlock --repo <tmp> --json --yes shared:dev-stack`

## Vision Target Delta

Primary tags: `CONTRACT`, `OPERATE`, `MAINT`

Baseline: broad unlock operations could clear many locks without an interactive
confirmation guard.

Current state: broad unlock operations follow the shared prompt policy and have
an explicit `--yes` automation path.

Remaining open: decide whether optional `init` starter selection still belongs
in this lane or whether `g03.027` should close.
