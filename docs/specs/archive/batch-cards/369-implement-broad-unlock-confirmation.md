# 369 - Implement Broad Unlock Confirmation

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Add confirmation before broad `effigy unlock` operations clear lock scopes.

## Scope

- add `--yes` to `effigy unlock`
- use the promoted `PromptPolicy`
- prompt only when stdin and stdout are real TTYs
- never prompt for `--json`
- preserve non-interactive safety:
  - fail clearly when confirmation is required but prompting is suppressed
  - allow automation only through explicit `--yes`
- require confirmation for:
  - `effigy unlock --all`
  - `effigy unlock workspace`
  - `effigy unlock shared:<name>`
  - any unlock invocation with more than one explicit scope
- do not require confirmation for one precise `task:<name>` or
  `profile:<task>/<profile>` scope
- prompt should show the target root and affected scope labels
- default to no
- update help/docs
- add targeted tests for:
  - parser support for `--yes`
  - prompt rendering and default-no cancellation
  - non-TTY suppression
  - `--json` suppression
  - `--yes` bypass
  - unprompted precise task/profile unlocks

## Non-Goals

- changing lock file semantics
- changing JSON envelope shape beyond parser/help support for `--yes`
- adding prompts to normal task acquisition
- implementing `init` starter selection

## Exit Condition

This card is complete when broad `unlock` actions follow the shared prompt
policy and have a documented `--yes` automation path.

## Closeout

Broad `unlock` actions now follow the shared prompt policy. `--all`,
`workspace`, `shared:<name>`, and multi-scope unlocks require confirmation in
eligible interactive flows and use `--yes` as the automation bypass. Precise
single `task:<name>` and `profile:<task>/<profile>` recovery unlocks remain
unprompted.

Validation:

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

## Next Card

[`370-decide-post-broad-unlock-confirmation-boundary.md`](./370-decide-post-broad-unlock-confirmation-boundary.md)
decides whether optional `init` starter selection is still warranted or the
lane can close.

## Next Task

Execute `370-decide-post-broad-unlock-confirmation-boundary.md`.
