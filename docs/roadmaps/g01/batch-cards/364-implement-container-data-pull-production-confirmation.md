# 364 - Implement Container Data Pull-Production Confirmation

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05

## Goal

Add the first destructive container/data prompt seam: confirmation before
`effigy container [<NAME>] data pull-production` runs.

## Scope

- add `--yes` to `container data pull-production`
- apply the shared prompt policy before pull-production runtime work starts
- prompt only when stdin and stdout are real TTYs
- never prompt for `--json`
- preserve non-interactive safety:
  - fail clearly when confirmation is required but prompting is suppressed
  - allow automation only through explicit `--yes`
- keep the confirmation bounded:
  - show the resolved container name when known
  - state that production data will be pulled into the local generated-compose
    environment
  - default to no
- update help/docs/changelog
- add targeted tests for:
  - prompt rendering and default-no cancellation
  - non-TTY suppression
  - `--json` suppression
  - `--yes` bypass

## Non-Goals

- `container data import`
- `unlock`
- broad prompt framework redesign
- changing pull-production hook semantics

## Exit Condition

This card is complete when `container data pull-production` follows the shared
prompt policy and has a documented `--yes` automation path.

## Closeout

Completed: 2026-05-05

- added `--yes` to `container data pull-production`
- added a default-no confirmation before production data pull side effects
- preserved script safety for `--json` and non-TTY execution
- kept Rhai `container::data("pull_production", ...)` non-interactive by using
  the explicit bypass internally
- updated help, docs, and changelog

Validation:

- `cargo check -p effigy-cli`
- `cargo check -p effigy`
- `cargo test -p effigy --lib parse_container_data_pull_production -- --nocapture`
- `cargo test -p effigy --lib container_data_pull_production_prompt -- --nocapture`
- `cargo test -p effigy --lib run_container_data_pull_production_rejects_direct_compose_ownership -- --nocapture`
- `cargo test -p effigy --lib prompt_container_data_pull_production -- --nocapture`

Format note: `cargo fmt --all -- --check` is blocked by pre-existing formatting
drift in unrelated dirty files.

## Next Card

After this lands, decide whether to widen to `container data import` or close
the container/data prompt subset first.
