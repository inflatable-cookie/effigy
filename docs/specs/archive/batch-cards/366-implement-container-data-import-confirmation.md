# 366 - Implement Container Data Import Confirmation

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Add confirmation before `effigy container [<NAME>] data import <VOLUME> <PATH>`
imports an archive into a generated-compose volume.

## Scope

- add `--yes` to `container data import`
- apply the shared prompt policy before import side effects start
- prompt only when stdin and stdout are real TTYs
- never prompt for `--json`
- preserve non-interactive safety:
  - fail clearly when confirmation is required but prompting is suppressed
  - allow automation only through explicit `--yes`
- keep the confirmation bounded:
  - show the resolved container name when known
  - show the target volume
  - show the resolved archive path
  - state that local generated-compose data may be overwritten
  - default to no
- update help/docs/changelog
- add targeted tests for:
  - parser support for `--yes`
  - prompt rendering and default-no cancellation
  - non-TTY suppression
  - `--json` suppression
  - `--yes` bypass

## Non-Goals

- `container data export`
- broad `unlock`
- runtime volume import semantics
- generic wizard framework

## Exit Condition

This card is complete when `container data import` follows the shared prompt
policy and has a documented `--yes` automation path.

## Next Card

[`367-decide-post-container-data-import-confirmation-boundary.md`](./367-decide-post-container-data-import-confirmation-boundary.md)
decides whether the lane should move directly to broad `unlock` confirmation.

## Closeout

`container data import` now follows the shared prompt policy before import side
effects start. It confirms generated-compose imports in real TTY flows,
defaults to no, suppresses prompts for `--json` and redirected I/O, and exposes
`--yes` as the explicit automation bypass.

Validation:

- `cargo check -p effigy-cli`
- `cargo check -p effigy`
- `cargo test -p effigy --lib parse_container_data_import -- --nocapture`
- `cargo test -p effigy --lib prompt_container_data_import -- --nocapture`
- `cargo test -p effigy --lib container_data_import_prompt -- --nocapture`
- `cargo test -p effigy --lib run_container_data_import_rejects_direct_compose_ownership -- --nocapture`
